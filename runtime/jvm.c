// The runtime of a Java program `ras java --c` wrote as C: what the program's source calls and
// does not define. It is the machine's own part (the heap, the classes, the exception pending,
// the method an object's class has) and the few classes of the library a program cannot be
// without: strings and their builder, the boxes, a list, the exceptions the machine itself
// throws, and the output streams. The interpreter's runtime (crates/ra-asm-jvm/src/run.rs) is the
// same machine over the IR; the two answer alike, and the fixtures hold them to it.
//
// A function here has the name the source calls it by: `jvm_new` for the machine's `jvm.new`,
// and for a method of the library the name crates/ra-asm-jvm/src/names.rs makes of
// `Class.name:descriptor`. A method an object's class finds is found by its `name:descriptor`
// in the class's table, the library's classes and the program's alike.
//
// An object is its class's number in its first eight bytes and its fields after them, as the
// lift lays it out; the eight bytes before it are the runtime's, and hold what an object of a
// library class is (a string's text, a list's elements, a box's value). An array is the number,
// its length at eight and its elements from sixteen. Nothing is collected.

#define _CRT_SECURE_NO_WARNINGS
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t *ref;

struct method {
    const char *named;
    void *function;
};

struct class {
    const char *name;
    const char *above;
    uint64_t fields;
    int interface_count, interface_room;
    const char **interfaces;
    int method_count, method_room;
    struct method *methods;
};

// A string's UTF-16 units, and a builder's.
struct text {
    uint32_t count, room;
    uint16_t *units;
};

struct list {
    uint32_t count, room;
    ref *items;
};

static struct class *classes;
static int class_count, class_room;
static ref pending;

#define NATIVE(object) (*(uint64_t *)((object) - 8))
#define NUMBER(object) (*(uint64_t *)(object))
#define LENGTH(array) (*(int32_t *)((array) + 8))
#define ELEMENTS 16

static void ready(void);

static void *room(size_t bytes) {
    void *at = calloc(1, bytes ? bytes : 1);
    if (!at) {
        fputs("jvm: out of memory\n", stderr);
        exit(70);
    }
    return at;
}

static void stop(const char *what, const char *name) {
    fprintf(stderr, "jvm: %s %s\n", what, name);
    exit(70);
}

static int find_class(const char *name) {
    for (int n = 0; n < class_count; n++) {
        if (strcmp(classes[n].name, name) == 0) return n;
    }
    return -1;
}

static struct class *define(const char *name, const char *above, uint64_t fields) {
    int n = find_class(name);
    if (n < 0) {
        if (class_count == class_room) {
            class_room = class_room ? class_room * 2 : 64;
            classes = realloc(classes, (size_t)class_room * sizeof *classes);
            if (!classes) stop("out of memory for the class", name);
        }
        n = class_count++;
    }
    // A program's own class of a library's name is the class.
    memset(&classes[n], 0, sizeof classes[n]);
    classes[n].name = name;
    classes[n].above = above;
    classes[n].fields = fields;
    return &classes[n];
}

// The number of the class `name`; an array's class is made when it is first named.
static int class_number(const char *name) {
    int n = find_class(name);
    if (n >= 0) return n;
    if (name[0] != '[') stop("the class is neither the program's nor one of the runtime's:", name);
    char *kept = room(strlen(name) + 1);
    strcpy(kept, name);
    define(kept, "java/lang/Object", 0);
    return find_class(kept);
}

// Whether the class `at` is `name`, or under it, or implements it.
static int is(const char *at, const char *name) {
    if (strcmp(at, name) == 0 || strcmp(name, "java/lang/Object") == 0) return 1;
    int n = find_class(at);
    if (n < 0) return 0;
    for (int i = 0; i < classes[n].interface_count; i++) {
        if (is(classes[n].interfaces[i], name)) return 1;
    }
    return classes[n].above && is(classes[n].above, name);
}

static const char *class_of(ref object) {
    uint64_t number = NUMBER(object);
    if (number == 0 || number > (uint64_t)class_count) stop("an object of no class", "");
    return classes[number - 1].name;
}

static ref allocate(const char *name, uint64_t bytes) {
    int n = class_number(name);
    ref object = (ref)room(8 + bytes) + 8;
    NUMBER(object) = (uint64_t)n + 1;
    return object;
}

static ref object(const char *name) {
    uint64_t bytes = 8;
    for (int n = find_class(name); n >= 0; n = classes[n].above ? find_class(classes[n].above) : -1) bytes += classes[n].fields;
    ref made = allocate(name, bytes);
    if (is(name, "java/lang/StringBuilder") || is(name, "java/lang/String")) {
        NATIVE(made) = (uint64_t)(uintptr_t)room(sizeof(struct text));
    } else if (is(name, "java/util/ArrayList")) {
        NATIVE(made) = (uint64_t)(uintptr_t)room(sizeof(struct list));
    }
    return made;
}

static struct text *text_of(ref object) {
    return (struct text *)(uintptr_t)NATIVE(object);
}

static void add_units(struct text *text, const uint16_t *units, uint32_t count) {
    if (text->count + count > text->room) {
        text->room = (text->count + count) * 2 + 16;
        text->units = realloc(text->units, text->room * sizeof *text->units);
        if (!text->units) stop("out of memory for", "a string");
    }
    if (count) memcpy(text->units + text->count, units, count * sizeof *units);
    text->count += count;
}

static void add_ascii(struct text *text, const char *ascii) {
    for (; *ascii; ascii++) {
        uint16_t unit = (uint8_t)*ascii;
        add_units(text, &unit, 1);
    }
}

static ref string_of(const uint16_t *units, uint32_t count) {
    ref made = object("java/lang/String");
    add_units(text_of(made), units, count);
    return made;
}

static ref string_ascii(const char *ascii) {
    ref made = object("java/lang/String");
    add_ascii(text_of(made), ascii);
    return made;
}

// Leaves an exception of the library's class `name` pending.
static void throw_new(const char *name, const char *message) {
    ref thrown = object(name);
    NATIVE(thrown) = message ? (uint64_t)(uintptr_t)string_ascii(message) : 0;
    pending = thrown;
}

static uint64_t nothing(void) {
    return 0;
}

// The function of the method `named` an object of the class `name` has: its class's, a
// superclass's, or the default of an interface.
static void *find_method(const char *name, const char *named) {
    for (int n = find_class(name); n >= 0; n = classes[n].above ? find_class(classes[n].above) : -1) {
        for (int m = 0; m < classes[n].method_count; m++) {
            if (strcmp(classes[n].methods[m].named, named) == 0) return classes[n].methods[m].function;
        }
    }
    for (int n = find_class(name); n >= 0; n = classes[n].above ? find_class(classes[n].above) : -1) {
        for (int i = 0; i < classes[n].interface_count; i++) {
            void *found = find_method(classes[n].interfaces[i], named);
            if (found) return found;
        }
    }
    return 0;
}

// The machine's own functions.

void jvm_define(uint8_t *name, uint8_t *above, uint64_t fields) {
    ready();
    define((const char *)name, (const char *)above, fields);
}

void jvm_implement(uint8_t *name, uint8_t *interface) {
    struct class *class = &classes[class_number((const char *)name)];
    if (class->interface_count == class->interface_room) {
        class->interface_room = class->interface_room * 2 + 4;
        class->interfaces = realloc(class->interfaces, (size_t)class->interface_room * sizeof *class->interfaces);
    }
    class->interfaces[class->interface_count++] = (const char *)interface;
}

void jvm_method(uint8_t *name, uint8_t *named, uint8_t *function) {
    struct class *class = &classes[class_number((const char *)name)];
    if (class->method_count == class->method_room) {
        class->method_room = class->method_room * 2 + 8;
        class->methods = realloc(class->methods, (size_t)class->method_room * sizeof *class->methods);
    }
    class->methods[class->method_count].named = (const char *)named;
    class->methods[class->method_count++].function = function;
}

uint8_t *jvm_pending(void) {
    return pending;
}

uint8_t *jvm_caught(void) {
    ref caught = pending;
    pending = 0;
    return caught;
}

void jvm_null(void) {
    throw_new("java/lang/NullPointerException", 0);
}

void jvm_throw(uint8_t *thrown) {
    if (thrown) {
        pending = thrown;
    } else {
        jvm_null();
    }
}

void jvm_monitorenter(uint8_t *object) {
    (void)object;
}

void jvm_monitorexit(uint8_t *object) {
    (void)object;
}

uint8_t *jvm_new(uint8_t *name) {
    return object((const char *)name);
}

// A string constant is one object wherever it is named: the lift names one text by one global.
uint8_t *jvm_string(uint8_t *units, uint32_t count) {
    static struct { uint8_t *units; ref string; } *constants;
    static int constant_count, constant_room;
    for (int n = 0; n < constant_count; n++) {
        if (constants[n].units == units) return constants[n].string;
    }
    if (constant_count == constant_room) {
        constant_room = constant_room * 2 + 64;
        constants = realloc(constants, (size_t)constant_room * sizeof *constants);
    }
    constants[constant_count].units = units;
    return constants[constant_count++].string = string_of((const uint16_t *)units, count);
}

uint8_t *jvm_class(uint8_t *name) {
    ref mirror = allocate("java/lang/Class", 8);
    NATIVE(mirror) = (uint64_t)(uintptr_t)name;
    return mirror;
}

// Where the static field `named` of a class of the library is.
uint8_t *jvm_static(uint8_t *named) {
    static ref out, err;
    static uint64_t unknown;
    ready();
    if (strcmp((const char *)named, "java/lang/System.out:Ljava/io/PrintStream;") == 0) {
        if (!out) {
            out = object("java/io/PrintStream");
            NATIVE(out) = 1;
        }
        return (uint8_t *)&out;
    }
    if (strcmp((const char *)named, "java/lang/System.err:Ljava/io/PrintStream;") == 0) {
        if (!err) {
            err = object("java/io/PrintStream");
            NATIVE(err) = 2;
        }
        return (uint8_t *)&err;
    }
    return (uint8_t *)&unknown;
}

uint8_t *jvm_virtual(uint8_t *receiver, uint8_t *named) {
    if (!receiver) {
        jvm_null();
        return (uint8_t *)nothing;
    }
    void *found = find_method(class_of(receiver), (const char *)named);
    if (!found) {
        fprintf(stderr, "jvm: the runtime has no method %s.%s\n", class_of(receiver), (const char *)named);
        exit(70);
    }
    return found;
}

uint32_t jvm_instanceof(uint8_t *object, uint8_t *name) {
    return object && is(class_of(object), (const char *)name);
}

static void dotted(char *out, size_t size, const char *name) {
    snprintf(out, size, "%s", name);
    for (; *out; out++) {
        if (*out == '/') *out = '.';
    }
}

void jvm_checkcast(uint8_t *object, uint8_t *name) {
    if (object && !is(class_of(object), (const char *)name)) {
        char from[256], to[256], message[600];
        dotted(from, sizeof from, class_of(object));
        dotted(to, sizeof to, (const char *)name);
        snprintf(message, sizeof message, "class %s cannot be cast to class %s", from, to);
        throw_new("java/lang/ClassCastException", message);
    }
}

uint8_t *jvm_element(uint8_t *array, uint32_t index, uint32_t size) {
    if (!array) {
        jvm_null();
        return 0;
    }
    if ((int32_t)index < 0 || (int32_t)index >= LENGTH(array)) {
        char message[80];
        snprintf(message, sizeof message, "Index %d out of bounds for length %d", (int32_t)index, LENGTH(array));
        throw_new("java/lang/ArrayIndexOutOfBoundsException", message);
        return 0;
    }
    return array + ELEMENTS + (uint64_t)size * index;
}

static uint64_t element_size(const char *descriptor) {
    switch (descriptor[1]) {
    case 'Z': case 'B': return 1;
    case 'C': case 'S': return 2;
    case 'I': case 'F': return 4;
    default: return 8;
    }
}

static ref array(const char *descriptor, int32_t count) {
    if (count < 0) {
        char message[16];
        snprintf(message, sizeof message, "%d", count);
        throw_new("java/lang/NegativeArraySizeException", message);
        return 0;
    }
    ref made = allocate(descriptor, ELEMENTS + element_size(descriptor) * (uint64_t)count);
    LENGTH(made) = count;
    return made;
}

uint8_t *jvm_newarray(uint32_t element, uint32_t count) {
    static const char *const descriptors[] = {"[Z", "[C", "[F", "[D", "[B", "[S", "[I", "[J"};
    return array(descriptors[element >= 4 && element <= 11 ? element - 4 : 7], (int32_t)count);
}

uint8_t *jvm_anewarray(uint8_t *name, uint32_t count) {
    char *descriptor = room(strlen((const char *)name) + 4);
    sprintf(descriptor, name[0] == '[' ? "[%s" : "[L%s;", (const char *)name);
    return array(descriptor, (int32_t)count);
}

static ref arrays(const char *descriptor, const int32_t *counts, uint32_t dimensions) {
    ref made = array(descriptor, counts[0]);
    if (made && dimensions > 1) {
        for (int32_t n = 0; n < counts[0]; n++) {
            ref inner = arrays(descriptor + 1, counts + 1, dimensions - 1);
            if (!inner) return 0;
            *(ref *)(made + ELEMENTS + 8 * (uint64_t)n) = inner;
        }
    }
    return made;
}

uint8_t *jvm_multianewarray(uint8_t *descriptor, uint32_t dimensions, uint8_t *counts) {
    for (uint32_t n = 0; n < dimensions; n++) {
        if (((const int32_t *)counts)[n] < 0) return array((const char *)descriptor, ((const int32_t *)counts)[n]);
    }
    return arrays((const char *)descriptor, (const int32_t *)counts, dimensions);
}

// A division by zero throws, and the least number by minus one is itself.
uint32_t jvm_idiv(uint32_t a, uint32_t b) {
    if (b == 0) return throw_new("java/lang/ArithmeticException", "/ by zero"), 0;
    return b == UINT32_MAX ? 0u - a : (uint32_t)((int32_t)a / (int32_t)b);
}

uint32_t jvm_irem(uint32_t a, uint32_t b) {
    if (b == 0) return throw_new("java/lang/ArithmeticException", "/ by zero"), 0;
    return b == UINT32_MAX ? 0 : (uint32_t)((int32_t)a % (int32_t)b);
}

uint64_t jvm_ldiv(uint64_t a, uint64_t b) {
    if (b == 0) return throw_new("java/lang/ArithmeticException", "/ by zero"), 0;
    return b == UINT64_MAX ? 0u - a : (uint64_t)((int64_t)a / (int64_t)b);
}

uint64_t jvm_lrem(uint64_t a, uint64_t b) {
    if (b == 0) return throw_new("java/lang/ArithmeticException", "/ by zero"), 0;
    return b == UINT64_MAX ? 0 : (uint64_t)((int64_t)a % (int64_t)b);
}

// A float that is no number is zero; one out of range is the nearest in it.
uint32_t jvm_d2i(double v) {
    if (v != v) return 0;
    if (v >= 2147483647.0) return (uint32_t)INT32_MAX;
    if (v <= -2147483648.0) return (uint32_t)INT32_MIN;
    return (uint32_t)(int32_t)v;
}

uint64_t jvm_d2l(double v) {
    if (v != v) return 0;
    if (v >= 9223372036854775807.0) return (uint64_t)INT64_MAX;
    if (v <= -9223372036854775808.0) return (uint64_t)INT64_MIN;
    return (uint64_t)(int64_t)v;
}

uint32_t jvm_f2i(float v) {
    return jvm_d2i(v);
}

uint64_t jvm_f2l(float v) {
    return jvm_d2l(v);
}

// The library.

// A `double` or a `float` as the library writes it: its shortest digits that read back as it,
// with a point and at least one digit after it, and in the scientific form outside a thousandth
// to ten million.
static void real_text(double v, int single, char *out) {
    if (v != v) {
        strcpy(out, "NaN");
        return;
    }
    if (isinf(v)) {
        strcpy(out, v > 0 ? "Infinity" : "-Infinity");
        return;
    }
    if (v == 0) {
        strcpy(out, signbit(v) ? "-0.0" : "0.0");
        return;
    }
    char scientific[48];
    for (int precision = 0; precision < 17; precision++) {
        snprintf(scientific, sizeof scientific, "%.*e", precision, v);
        if (single ? strtof(scientific, 0) == (float)v : strtod(scientific, 0) == v) break;
    }
    const char *at = scientific;
    if (*at == '-') {
        *out++ = '-';
        at++;
    }
    char digits[24];
    int count = 0;
    for (; *at && *at != 'e'; at++) {
        if (*at != '.') digits[count++] = *at;
    }
    int exponent = atoi(at + 1);
    if (exponent >= -3 && exponent < 7) {
        if (exponent < 0) {
            *out++ = '0';
            *out++ = '.';
            for (int n = 0; n < -exponent - 1; n++) *out++ = '0';
            memcpy(out, digits, (size_t)count);
            out += count;
        } else {
            for (int n = 0; n <= exponent; n++) *out++ = n < count ? digits[n] : '0';
            *out++ = '.';
            if (count > exponent + 1) {
                memcpy(out, digits + exponent + 1, (size_t)(count - exponent - 1));
                out += count - exponent - 1;
            } else {
                *out++ = '0';
            }
        }
        *out = 0;
        return;
    }
    *out++ = digits[0];
    *out++ = '.';
    if (count > 1) {
        memcpy(out, digits + 1, (size_t)(count - 1));
        out += count - 1;
    } else {
        *out++ = '0';
    }
    sprintf(out, "E%d", exponent);
}

// What `String.valueOf` makes of an object.
static ref describe(ref object) {
    if (!object) return string_ascii("null");
    if (strcmp(class_of(object), "java/lang/String") == 0) return object;
    ref (*to_string)(ref) = (ref (*)(ref))jvm_virtual(object, (uint8_t *)"toString:()Ljava/lang/String;");
    ref made = to_string(object);
    return made ? made : string_ascii("null");
}

// What is thrown as the machine names it at the end of a thread: `class: message`.
static void thrown_text(ref thrown, struct text *out) {
    char name[256];
    dotted(name, sizeof name, class_of(thrown));
    add_ascii(out, name);
    ref message = (ref)(uintptr_t)NATIVE(thrown);
    if (message) {
        add_ascii(out, ": ");
        add_units(out, text_of(message)->units, text_of(message)->count);
    }
}

void java_lang_Object__new___V(ref this) {
    (void)this;
}

static uint32_t Object_hashCode(ref this) {
    return (uint32_t)((uintptr_t)this >> 3);
}

static uint32_t Object_equals(ref this, ref other) {
    return this == other;
}

static ref Object_toString(ref this) {
    char name[300];
    dotted(name, sizeof name, class_of(this));
    snprintf(name + strlen(name), 20, "@%x", Object_hashCode(this));
    return string_ascii(name);
}

static uint32_t String_length(ref this) {
    return text_of(this)->count;
}

static uint32_t String_charAt(ref this, uint32_t index) {
    if (index >= text_of(this)->count) {
        char message[40];
        snprintf(message, sizeof message, "index %d", (int32_t)index);
        throw_new("java/lang/StringIndexOutOfBoundsException", message);
        return 0;
    }
    return text_of(this)->units[index];
}

static uint32_t String_isEmpty(ref this) {
    return text_of(this)->count == 0;
}

static uint32_t String_hashCode(ref this) {
    uint32_t hash = 0;
    for (uint32_t n = 0; n < text_of(this)->count; n++) hash = hash * 31 + text_of(this)->units[n];
    return hash;
}

static uint32_t String_equals(ref this, ref other) {
    if (!other || strcmp(class_of(other), "java/lang/String") != 0) return 0;
    struct text *a = text_of(this), *b = text_of(other);
    return a->count == b->count && (a->count == 0 || memcmp(a->units, b->units, a->count * sizeof *a->units) == 0);
}

static ref String_toString(ref this) {
    return this;
}

ref java_lang_String__valueOf__Ljava_lang_Object_2_Ljava_lang_String_2(ref object) {
    return describe(object);
}

ref java_lang_String__valueOf__I_Ljava_lang_String_2(uint32_t v) {
    char digits[16];
    snprintf(digits, sizeof digits, "%d", (int32_t)v);
    return string_ascii(digits);
}

ref java_lang_Objects__requireNonNull__Ljava_lang_Object_2_Ljava_lang_Object_2(ref object) {
    if (!object) jvm_null();
    return object;
}

void java_lang_StringBuilder__new___V(ref this) {
    (void)this;
}

void java_lang_StringBuilder__new__Ljava_lang_String_2_V(ref this, ref from) {
    if (from) add_units(text_of(this), text_of(from)->units, text_of(from)->count);
}

ref java_lang_StringBuilder__append__Ljava_lang_String_2_Ljava_lang_StringBuilder_2(ref this, ref added) {
    if (added) {
        add_units(text_of(this), text_of(added)->units, text_of(added)->count);
    } else {
        add_ascii(text_of(this), "null");
    }
    return this;
}

ref java_lang_StringBuilder__append__Ljava_lang_Object_2_Ljava_lang_StringBuilder_2(ref this, ref added) {
    return java_lang_StringBuilder__append__Ljava_lang_String_2_Ljava_lang_StringBuilder_2(this, describe(added));
}

ref java_lang_StringBuilder__append__I_Ljava_lang_StringBuilder_2(ref this, uint32_t v) {
    char digits[16];
    snprintf(digits, sizeof digits, "%d", (int32_t)v);
    add_ascii(text_of(this), digits);
    return this;
}

ref java_lang_StringBuilder__append__J_Ljava_lang_StringBuilder_2(ref this, uint64_t v) {
    char digits[24];
    snprintf(digits, sizeof digits, "%lld", (long long)v);
    add_ascii(text_of(this), digits);
    return this;
}

ref java_lang_StringBuilder__append__C_Ljava_lang_StringBuilder_2(ref this, uint32_t v) {
    uint16_t unit = (uint16_t)v;
    add_units(text_of(this), &unit, 1);
    return this;
}

ref java_lang_StringBuilder__append__Z_Ljava_lang_StringBuilder_2(ref this, uint32_t v) {
    add_ascii(text_of(this), v & 1 ? "true" : "false");
    return this;
}

ref java_lang_StringBuilder__append__F_Ljava_lang_StringBuilder_2(ref this, float v) {
    char digits[48];
    real_text(v, 1, digits);
    add_ascii(text_of(this), digits);
    return this;
}

ref java_lang_StringBuilder__append__D_Ljava_lang_StringBuilder_2(ref this, double v) {
    char digits[48];
    real_text(v, 0, digits);
    add_ascii(text_of(this), digits);
    return this;
}

ref java_lang_StringBuilder__toString___Ljava_lang_String_2(ref this) {
    return string_of(text_of(this)->units, text_of(this)->count);
}

// Writes UTF-16 units to the stream as UTF-8.
static void write_units(ref stream, const uint16_t *units, uint32_t count, int line) {
    FILE *file = NATIVE(stream) == 2 ? stderr : stdout;
    for (uint32_t n = 0; n < count; n++) {
        uint32_t c = units[n];
        if (c >= 0xd800 && c < 0xdc00 && n + 1 < count && units[n + 1] >= 0xdc00 && units[n + 1] < 0xe000) {
            c = 0x10000 + ((c - 0xd800) << 10) + (units[++n] - 0xdc00);
        }
        if (c < 0x80) {
            fputc((int)c, file);
        } else if (c < 0x800) {
            fputc((int)(0xc0 | c >> 6), file);
            fputc((int)(0x80 | (c & 0x3f)), file);
        } else if (c < 0x10000) {
            fputc((int)(0xe0 | c >> 12), file);
            fputc((int)(0x80 | (c >> 6 & 0x3f)), file);
            fputc((int)(0x80 | (c & 0x3f)), file);
        } else {
            fputc((int)(0xf0 | c >> 18), file);
            fputc((int)(0x80 | (c >> 12 & 0x3f)), file);
            fputc((int)(0x80 | (c >> 6 & 0x3f)), file);
            fputc((int)(0x80 | (c & 0x3f)), file);
        }
    }
    if (line) fputc('\n', file);
}

static void write_ascii(ref stream, const char *ascii, int line) {
    FILE *file = NATIVE(stream) == 2 ? stderr : stdout;
    fputs(ascii, file);
    if (line) fputc('\n', file);
}

// Each form of `print` and of `println`, by what it takes.
#define PRINTS(suffix, line) \
    static void PrintStream_##suffix##_V(ref this) { write_ascii(this, "", line); } \
    static void PrintStream_##suffix##_I(ref this, uint32_t v) { char d[16]; snprintf(d, sizeof d, "%d", (int32_t)v); write_ascii(this, d, line); } \
    static void PrintStream_##suffix##_J(ref this, uint64_t v) { char d[24]; snprintf(d, sizeof d, "%lld", (long long)v); write_ascii(this, d, line); } \
    static void PrintStream_##suffix##_C(ref this, uint32_t v) { uint16_t unit = (uint16_t)v; write_units(this, &unit, 1, line); } \
    static void PrintStream_##suffix##_Z(ref this, uint32_t v) { write_ascii(this, v & 1 ? "true" : "false", line); } \
    static void PrintStream_##suffix##_F(ref this, float v) { char d[48]; real_text(v, 1, d); write_ascii(this, d, line); } \
    static void PrintStream_##suffix##_D(ref this, double v) { char d[48]; real_text(v, 0, d); write_ascii(this, d, line); } \
    static void PrintStream_##suffix##_Object(ref this, ref v) { \
        ref string = describe(v); \
        write_units(this, text_of(string)->units, text_of(string)->count, line); \
    }
PRINTS(print, 0)
PRINTS(println, 1)

void java_util_ArrayList__new___V(ref this) {
    (void)this;
}

static struct list *list_of(ref object) {
    return (struct list *)(uintptr_t)NATIVE(object);
}

static uint32_t ArrayList_add(ref this, ref item) {
    struct list *list = list_of(this);
    if (list->count == list->room) {
        list->room = list->room * 2 + 8;
        list->items = realloc(list->items, list->room * sizeof *list->items);
        if (!list->items) stop("out of memory for", "a list");
    }
    list->items[list->count++] = item;
    return 1;
}

static uint32_t ArrayList_size(ref this) {
    return list_of(this)->count;
}

static uint32_t ArrayList_isEmpty(ref this) {
    return list_of(this)->count == 0;
}

static ref ArrayList_get(ref this, uint32_t index) {
    if (index >= list_of(this)->count) {
        char message[80];
        snprintf(message, sizeof message, "Index %d out of bounds for length %u", (int32_t)index, list_of(this)->count);
        throw_new("java/lang/IndexOutOfBoundsException", message);
        return 0;
    }
    return list_of(this)->items[index];
}

// An exception's constructors: with nothing, and with its message.
#define THROWABLE(name) \
    void name##__new___V(ref this) { NATIVE(this) = 0; } \
    void name##__new__Ljava_lang_String_2_V(ref this, ref message) { NATIVE(this) = (uint64_t)(uintptr_t)message; }
THROWABLE(java_lang_Throwable)
THROWABLE(java_lang_Error)
THROWABLE(java_lang_Exception)
THROWABLE(java_lang_RuntimeException)
THROWABLE(java_lang_IllegalStateException)
THROWABLE(java_lang_IllegalArgumentException)
THROWABLE(java_lang_UnsupportedOperationException)
THROWABLE(java_lang_ArithmeticException)
THROWABLE(java_lang_NullPointerException)
THROWABLE(java_lang_ClassCastException)
THROWABLE(java_lang_IndexOutOfBoundsException)
THROWABLE(java_lang_ArrayIndexOutOfBoundsException)

static ref Throwable_getMessage(ref this) {
    return (ref)(uintptr_t)NATIVE(this);
}

static ref Throwable_toString(ref this) {
    ref made = object("java/lang/String");
    thrown_text(this, text_of(made));
    return made;
}

// A box holds its value's bits.
static ref boxed(const char *name, uint64_t bits) {
    ref made = allocate(name, 8);
    NATIVE(made) = bits;
    return made;
}

ref java_lang_Integer__valueOf__I_Ljava_lang_Integer_2(uint32_t v) {
    return boxed("java/lang/Integer", v);
}

ref java_lang_Long__valueOf__J_Ljava_lang_Long_2(uint64_t v) {
    return boxed("java/lang/Long", v);
}

ref java_lang_Boolean__valueOf__Z_Ljava_lang_Boolean_2(uint32_t v) {
    return boxed("java/lang/Boolean", v & 1);
}

ref java_lang_Character__valueOf__C_Ljava_lang_Character_2(uint32_t v) {
    return boxed("java/lang/Character", v & 0xffff);
}

ref java_lang_Double__valueOf__D_Ljava_lang_Double_2(double v) {
    uint64_t bits;
    memcpy(&bits, &v, sizeof bits);
    return boxed("java/lang/Double", bits);
}

static uint32_t Box_intValue(ref this) {
    return (uint32_t)NATIVE(this);
}

static uint64_t Long_longValue(ref this) {
    return NATIVE(this);
}

static double Double_doubleValue(ref this) {
    double v;
    memcpy(&v, &NATIVE(this), sizeof v);
    return v;
}

static ref Integer_toString(ref this) {
    return java_lang_String__valueOf__I_Ljava_lang_String_2((uint32_t)NATIVE(this));
}

static void library(const char *name, const char *above, const char *first, const char *second) {
    define(name, above, 0);
    if (first) jvm_implement((uint8_t *)name, (uint8_t *)first);
    if (second) jvm_implement((uint8_t *)name, (uint8_t *)second);
}

static void method(const char *name, const char *named, void *function) {
    jvm_method((uint8_t *)name, (uint8_t *)named, function);
}

// The library's classes and the methods an object's class finds.
static void ready(void) {
    static int done;
    if (done) return;
    done = 1;
    library("java/lang/Object", 0, 0, 0);
    method("java/lang/Object", "hashCode:()I", (void *)Object_hashCode);
    method("java/lang/Object", "equals:(Ljava/lang/Object;)Z", (void *)Object_equals);
    method("java/lang/Object", "toString:()Ljava/lang/String;", (void *)Object_toString);
    library("java/lang/String", "java/lang/Object", "java/lang/CharSequence", "java/lang/Comparable");
    method("java/lang/String", "length:()I", (void *)String_length);
    method("java/lang/String", "charAt:(I)C", (void *)String_charAt);
    method("java/lang/String", "isEmpty:()Z", (void *)String_isEmpty);
    method("java/lang/String", "hashCode:()I", (void *)String_hashCode);
    method("java/lang/String", "equals:(Ljava/lang/Object;)Z", (void *)String_equals);
    method("java/lang/String", "toString:()Ljava/lang/String;", (void *)String_toString);
    library("java/lang/StringBuilder", "java/lang/Object", "java/lang/CharSequence", 0);
    const char *builder = "java/lang/StringBuilder";
    method(builder, "length:()I", (void *)String_length);
    method(builder, "charAt:(I)C", (void *)String_charAt);
    method(builder, "toString:()Ljava/lang/String;", (void *)java_lang_StringBuilder__toString___Ljava_lang_String_2);
    method(builder, "append:(Ljava/lang/String;)Ljava/lang/StringBuilder;", (void *)java_lang_StringBuilder__append__Ljava_lang_String_2_Ljava_lang_StringBuilder_2);
    method(builder, "append:(Ljava/lang/Object;)Ljava/lang/StringBuilder;", (void *)java_lang_StringBuilder__append__Ljava_lang_Object_2_Ljava_lang_StringBuilder_2);
    method(builder, "append:(I)Ljava/lang/StringBuilder;", (void *)java_lang_StringBuilder__append__I_Ljava_lang_StringBuilder_2);
    method(builder, "append:(J)Ljava/lang/StringBuilder;", (void *)java_lang_StringBuilder__append__J_Ljava_lang_StringBuilder_2);
    method(builder, "append:(C)Ljava/lang/StringBuilder;", (void *)java_lang_StringBuilder__append__C_Ljava_lang_StringBuilder_2);
    method(builder, "append:(Z)Ljava/lang/StringBuilder;", (void *)java_lang_StringBuilder__append__Z_Ljava_lang_StringBuilder_2);
    method(builder, "append:(F)Ljava/lang/StringBuilder;", (void *)java_lang_StringBuilder__append__F_Ljava_lang_StringBuilder_2);
    method(builder, "append:(D)Ljava/lang/StringBuilder;", (void *)java_lang_StringBuilder__append__D_Ljava_lang_StringBuilder_2);
    library("java/lang/Class", "java/lang/Object", 0, 0);
    library("java/io/PrintStream", "java/lang/Object", 0, 0);
    const char *stream = "java/io/PrintStream";
    method(stream, "print:()V", (void *)PrintStream_print_V);
    method(stream, "print:(I)V", (void *)PrintStream_print_I);
    method(stream, "print:(J)V", (void *)PrintStream_print_J);
    method(stream, "print:(C)V", (void *)PrintStream_print_C);
    method(stream, "print:(Z)V", (void *)PrintStream_print_Z);
    method(stream, "print:(F)V", (void *)PrintStream_print_F);
    method(stream, "print:(D)V", (void *)PrintStream_print_D);
    method(stream, "print:(Ljava/lang/String;)V", (void *)PrintStream_print_Object);
    method(stream, "print:(Ljava/lang/Object;)V", (void *)PrintStream_print_Object);
    method(stream, "println:()V", (void *)PrintStream_println_V);
    method(stream, "println:(I)V", (void *)PrintStream_println_I);
    method(stream, "println:(J)V", (void *)PrintStream_println_J);
    method(stream, "println:(C)V", (void *)PrintStream_println_C);
    method(stream, "println:(Z)V", (void *)PrintStream_println_Z);
    method(stream, "println:(F)V", (void *)PrintStream_println_F);
    method(stream, "println:(D)V", (void *)PrintStream_println_D);
    method(stream, "println:(Ljava/lang/String;)V", (void *)PrintStream_println_Object);
    method(stream, "println:(Ljava/lang/Object;)V", (void *)PrintStream_println_Object);
    library("java/util/ArrayList", "java/lang/Object", "java/util/List", "java/util/Collection");
    method("java/util/ArrayList", "add:(Ljava/lang/Object;)Z", (void *)ArrayList_add);
    method("java/util/ArrayList", "size:()I", (void *)ArrayList_size);
    method("java/util/ArrayList", "isEmpty:()Z", (void *)ArrayList_isEmpty);
    method("java/util/ArrayList", "get:(I)Ljava/lang/Object;", (void *)ArrayList_get);
    library("java/lang/Throwable", "java/lang/Object", 0, 0);
    method("java/lang/Throwable", "getMessage:()Ljava/lang/String;", (void *)Throwable_getMessage);
    method("java/lang/Throwable", "toString:()Ljava/lang/String;", (void *)Throwable_toString);
    library("java/lang/Error", "java/lang/Throwable", 0, 0);
    library("java/lang/Exception", "java/lang/Throwable", 0, 0);
    library("java/lang/RuntimeException", "java/lang/Exception", 0, 0);
    library("java/lang/IllegalStateException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/IllegalArgumentException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/UnsupportedOperationException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/ArithmeticException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/NullPointerException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/ClassCastException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/NegativeArraySizeException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/IndexOutOfBoundsException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/ArrayIndexOutOfBoundsException", "java/lang/IndexOutOfBoundsException", 0, 0);
    library("java/lang/StringIndexOutOfBoundsException", "java/lang/IndexOutOfBoundsException", 0, 0);
    library("java/lang/Number", "java/lang/Object", 0, 0);
    library("java/lang/Integer", "java/lang/Number", "java/lang/Comparable", 0);
    method("java/lang/Integer", "intValue:()I", (void *)Box_intValue);
    method("java/lang/Integer", "toString:()Ljava/lang/String;", (void *)Integer_toString);
    library("java/lang/Long", "java/lang/Number", "java/lang/Comparable", 0);
    method("java/lang/Long", "longValue:()J", (void *)Long_longValue);
    library("java/lang/Double", "java/lang/Number", "java/lang/Comparable", 0);
    method("java/lang/Double", "doubleValue:()D", (void *)Double_doubleValue);
    library("java/lang/Boolean", "java/lang/Object", "java/lang/Comparable", 0);
    method("java/lang/Boolean", "booleanValue:()Z", (void *)Box_intValue);
    library("java/lang/Character", "java/lang/Object", "java/lang/Comparable", 0);
    method("java/lang/Character", "charValue:()C", (void *)Box_intValue);
}

// The program's entry, which the source's `main` calls around the program's own.

// The array of strings a program's `main` takes: the command's arguments after its name.
uint8_t *jvm_arguments(uint32_t count, uint8_t *arguments) {
    ready();
    int32_t given = (int32_t)count > 1 ? (int32_t)count - 1 : 0;
    ref made = array("[Ljava/lang/String;", given);
    for (int32_t n = 0; n < given; n++) {
        *(ref *)(made + ELEMENTS + 8 * (uint64_t)n) = string_ascii(((char **)arguments)[n + 1]);
    }
    return made;
}

// The end of the program: the exception nothing caught is named on the error stream, and the
// process's exit code says there was one.
uint32_t jvm_end(void) {
    fflush(stdout);
    if (!pending) return 0;
    struct text named = {0};
    thrown_text(pending, &named);
    fputs("Exception in thread \"main\" ", stderr);
    ref err = *(ref *)jvm_static((uint8_t *)"java/lang/System.err:Ljava/io/PrintStream;");
    write_units(err, named.units, named.count, 1);
    return 1;
}

// The source's own entry (`ras java --c FILE --main CLASS` writes it): the classes told, their
// static initializers run, and the class's `main` called.
extern uint32_t jvm_main(uint32_t count, uint8_t *arguments);

int main(int count, char **arguments) {
    return (int)jvm_main((uint32_t)count, (uint8_t *)arguments);
}
