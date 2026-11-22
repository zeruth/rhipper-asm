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
#include <ctype.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wctype.h>

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
    } else if (is(name, "java/util/HashMap") || is(name, "java/util/HashSet")) {
        // A map is two lists, its keys and their values, and the most it has held; a set is a
        // map of its elements.
        NATIVE(made) = (uint64_t)(uintptr_t)room(2 * sizeof(struct list) + 16);
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

ref java_util_Objects__requireNonNull__Ljava_lang_Object_2_Ljava_lang_Object_2(ref object) {
    if (!object) jvm_null();
    return object;
}

uint32_t java_util_Objects__equals__Ljava_lang_Object_2Ljava_lang_Object_2_Z(ref a, ref b) {
    if (a == b) return 1;
    if (!a) return 0;
    return ((uint32_t (*)(ref, ref))jvm_virtual(a, (uint8_t *)"equals:(Ljava/lang/Object;)Z"))(a, b) & 1;
}

uint32_t java_util_Objects__hashCode__Ljava_lang_Object_2_I(ref object) {
    return object ? ((uint32_t (*)(ref))jvm_virtual(object, (uint8_t *)"hashCode:()I"))(object) : 0;
}

uint32_t java_lang_Math__abs__I_I(uint32_t v) {
    return (int32_t)v < 0 ? 0u - v : v;
}

void java_lang_Record__new___V(ref this) {
    (void)this;
}

// An array's copy: the same class, the same length, the same elements. No other object of the
// library is copied.
static ref Object_clone(ref this) {
    const char *name = class_of(this);
    if (name[0] != '[') {
        throw_new("java/lang/CloneNotSupportedException", name);
        return 0;
    }
    uint64_t bytes = ELEMENTS + element_size(name) * (uint64_t)LENGTH(this);
    ref copy = allocate(name, bytes);
    memcpy(copy + 8, this + 8, bytes - 8);
    return copy;
}

// A constant of an enum: its name and its place among the constants. The runtime keeps every
// constant made, for `valueOf` to find one by its class and its name.
struct constant {
    ref name;
    int32_t ordinal;
};

static ref *constants;
static int constant_count, constant_room;

static struct constant *constant_of(ref object) {
    return (struct constant *)(uintptr_t)NATIVE(object);
}

void java_lang_Enum__new__Ljava_lang_String_2I_V(ref this, ref name, uint32_t ordinal) {
    struct constant *constant = room(sizeof *constant);
    constant->name = name;
    constant->ordinal = (int32_t)ordinal;
    NATIVE(this) = (uint64_t)(uintptr_t)constant;
    if (constant_count == constant_room) {
        constant_room = constant_room * 2 + 16;
        constants = realloc(constants, (size_t)constant_room * sizeof *constants);
        if (!constants) stop("out of memory for", "an enum");
    }
    constants[constant_count++] = this;
}

static ref Enum_name(ref this) {
    return constant_of(this)->name;
}

static uint32_t Enum_ordinal(ref this) {
    return (uint32_t)constant_of(this)->ordinal;
}

static uint32_t Enum_compareTo(ref this, ref other) {
    return (uint32_t)(constant_of(this)->ordinal - constant_of(other)->ordinal);
}

static uint32_t String_equals(ref this, ref other);

ref java_lang_Enum__valueOf__Ljava_lang_Class_2Ljava_lang_String_2_Ljava_lang_Enum_2(ref mirror, ref name) {
    const char *of = (const char *)(uintptr_t)NATIVE(mirror);
    for (int n = 0; n < constant_count; n++) {
        if (strcmp(class_of(constants[n]), of) == 0 && String_equals(constant_of(constants[n])->name, name)) return constants[n];
    }
    // `No enum constant pkg.Outer.Enum.NAME`
    char message[400];
    int at = snprintf(message, sizeof message - 2, "No enum constant %s.", of);
    for (char *c = message + 17; *c; c++) {
        if (*c == '/' || *c == '$') *c = '.';
    }
    for (uint32_t n = 0; name && n < text_of(name)->count && at < (int)sizeof message - 1; n++) message[at++] = (char)text_of(name)->units[n];
    message[at] = 0;
    throw_new("java/lang/IllegalArgumentException", message);
    return 0;
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

static uint32_t ArrayList_contains(ref this, ref item) {
    for (uint32_t n = 0; n < list_of(this)->count; n++) {
        if (java_util_Objects__equals__Ljava_lang_Object_2Ljava_lang_Object_2_Z(item, list_of(this)->items[n])) return 1;
    }
    return 0;
}

// `[first, second]`, each element as `String.valueOf` writes it.
static ref ArrayList_toString(ref this) {
    ref made = object("java/lang/String");
    add_ascii(text_of(made), "[");
    for (uint32_t n = 0; n < list_of(this)->count; n++) {
        if (n) add_ascii(text_of(made), ", ");
        ref item = describe(list_of(this)->items[n]);
        add_units(text_of(made), text_of(item)->units, text_of(item)->count);
    }
    add_ascii(text_of(made), "]");
    return made;
}

// Where an iterator of a list is.
struct iterator {
    ref list;
    uint32_t at;
};

static ref ArrayList_iterator(ref this) {
    ref made = allocate("java/util/ArrayList$Itr", 8);
    struct iterator *iterator = room(sizeof *iterator);
    iterator->list = this;
    NATIVE(made) = (uint64_t)(uintptr_t)iterator;
    return made;
}

static uint32_t Iterator_hasNext(ref this) {
    struct iterator *iterator = (struct iterator *)(uintptr_t)NATIVE(this);
    return iterator->at < list_of(iterator->list)->count;
}

static ref Iterator_next(ref this) {
    struct iterator *iterator = (struct iterator *)(uintptr_t)NATIVE(this);
    return iterator->at < list_of(iterator->list)->count ? list_of(iterator->list)->items[iterator->at++] : 0;
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

THROWABLE(java_lang_MatchException)

// A switch that no case took: its message, and what was thrown while it looked.
void java_lang_MatchException__new__Ljava_lang_String_2Ljava_lang_Throwable_2_V(ref this, ref message, ref cause) {
    (void)cause;
    NATIVE(this) = (uint64_t)(uintptr_t)message;
}

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

static ref Long_toString(ref this) {
    char digits[24];
    snprintf(digits, sizeof digits, "%lld", (long long)NATIVE(this));
    return string_ascii(digits);
}

static ref Double_toString(ref this) {
    char digits[48];
    real_text(Double_doubleValue(this), 0, digits);
    return string_ascii(digits);
}

static ref Boolean_toString(ref this) {
    return string_ascii(NATIVE(this) & 1 ? "true" : "false");
}

static ref Character_toString(ref this) {
    uint16_t unit = (uint16_t)NATIVE(this);
    return string_of(&unit, 1);
}

// Two boxes are equal when they are of one class and hold one value.
static uint32_t Box_equals(ref this, ref other) {
    return other && NUMBER(other) == NUMBER(this) && NATIVE(other) == NATIVE(this);
}

static uint32_t Box_hashCode(ref this) {
    return (uint32_t)(NATIVE(this) ^ NATIVE(this) >> 32);
}

// The everyday library: what strings and their builder do, numbers read from text, characters
// told apart, a list changed in place, a map, and arrays copied.

static int32_t find_units(struct text *text, const uint16_t *part, uint32_t count) {
    for (uint32_t at = 0; at + count <= text->count; at++) {
        if (count == 0 || memcmp(text->units + at, part, count * sizeof *part) == 0) return (int32_t)at;
    }
    return -1;
}

static uint32_t String_indexOfChar(ref this, uint32_t unit) {
    for (uint32_t n = 0; n < text_of(this)->count; n++) {
        if (text_of(this)->units[n] == (uint16_t)unit) return n;
    }
    return (uint32_t)-1;
}

static uint32_t String_lastIndexOfChar(ref this, uint32_t unit) {
    for (uint32_t n = text_of(this)->count; n > 0; n--) {
        if (text_of(this)->units[n - 1] == (uint16_t)unit) return n - 1;
    }
    return (uint32_t)-1;
}

static uint32_t String_indexOf(ref this, ref part) {
    part = describe(part);
    return (uint32_t)find_units(text_of(this), text_of(part)->units, text_of(part)->count);
}

static uint32_t String_contains(ref this, ref part) {
    return (int32_t)String_indexOf(this, part) >= 0;
}

static uint32_t String_startsWith(ref this, ref part) {
    struct text *a = text_of(this), *b = text_of(part);
    return b->count <= a->count && (b->count == 0 || memcmp(a->units, b->units, b->count * sizeof *b->units) == 0);
}

static uint32_t String_endsWith(ref this, ref part) {
    struct text *a = text_of(this), *b = text_of(part);
    return b->count <= a->count && (b->count == 0 || memcmp(a->units + a->count - b->count, b->units, b->count * sizeof *b->units) == 0);
}

static ref String_replace(ref this, uint32_t from, uint32_t to) {
    ref made = string_of(text_of(this)->units, text_of(this)->count);
    for (uint32_t n = 0; n < text_of(made)->count; n++) {
        if (text_of(made)->units[n] == (uint16_t)from) text_of(made)->units[n] = (uint16_t)to;
    }
    return made;
}

// A character's other case, where that is one character of the first plane.
static uint16_t upper(uint16_t unit) {
    return (uint16_t)towupper(unit);
}

static uint16_t lower(uint16_t unit) {
    return (uint16_t)towlower(unit);
}

static ref String_toUpperCase(ref this) {
    ref made = string_of(text_of(this)->units, text_of(this)->count);
    for (uint32_t n = 0; n < text_of(made)->count; n++) text_of(made)->units[n] = upper(text_of(made)->units[n]);
    return made;
}

static ref String_toLowerCase(ref this) {
    ref made = string_of(text_of(this)->units, text_of(this)->count);
    for (uint32_t n = 0; n < text_of(made)->count; n++) text_of(made)->units[n] = lower(text_of(made)->units[n]);
    return made;
}

// What is trimmed is every character up to the space, as the library has it.
static ref String_trim(ref this) {
    struct text *text = text_of(this);
    uint32_t from = 0, to = text->count;
    while (from < to && text->units[from] <= ' ') from++;
    while (to > from && text->units[to - 1] <= ' ') to--;
    return string_of(text->units + from, to - from);
}

static ref substring(ref this, int32_t from, int32_t to) {
    int32_t count = (int32_t)text_of(this)->count;
    if (from < 0 || to > count || from > to) {
        char message[80];
        snprintf(message, sizeof message, "begin %d, end %d, length %d", from, to, count);
        throw_new("java/lang/StringIndexOutOfBoundsException", message);
        return 0;
    }
    return string_of(text_of(this)->units + from, (uint32_t)(to - from));
}

static ref String_substring(ref this, uint32_t from) {
    return substring(this, (int32_t)from, (int32_t)text_of(this)->count);
}

static ref String_substring2(ref this, uint32_t from, uint32_t to) {
    return substring(this, (int32_t)from, (int32_t)to);
}

static uint32_t String_compareTo(ref this, ref other) {
    struct text *a = text_of(this), *b = text_of(other);
    for (uint32_t n = 0; n < a->count && n < b->count; n++) {
        if (a->units[n] != b->units[n]) return (uint32_t)((int32_t)a->units[n] - (int32_t)b->units[n]);
    }
    return (uint32_t)((int32_t)a->count - (int32_t)b->count);
}

static ref String_toCharArray(ref this) {
    ref made = array("[C", (int32_t)text_of(this)->count);
    if (text_of(this)->count) memcpy(made + ELEMENTS, text_of(this)->units, text_of(this)->count * 2);
    return made;
}

void java_lang_String__new___3C_V(ref this, ref characters) {
    if (!characters) {
        jvm_null();
        return;
    }
    add_units(text_of(this), (const uint16_t *)(characters + ELEMENTS), (uint32_t)LENGTH(characters));
}

ref java_lang_String__valueOf___3CII_Ljava_lang_String_2(ref characters, uint32_t from, uint32_t count) {
    if (!characters) {
        jvm_null();
        return 0;
    }
    if ((uint64_t)from + count > (uint64_t)LENGTH(characters)) {
        throw_new("java/lang/StringIndexOutOfBoundsException", "a run outside the array");
        return 0;
    }
    return string_of((const uint16_t *)(characters + ELEMENTS) + from, count);
}

ref java_lang_String__valueOf__F_Ljava_lang_String_2(float v) {
    char digits[48];
    real_text(v, 1, digits);
    return string_ascii(digits);
}

static ref StringBuilder_insert(ref this, uint32_t at, ref added) {
    struct text *text = text_of(this);
    added = describe(added);
    uint32_t count = text_of(added)->count;
    if (at > text->count) {
        throw_new("java/lang/StringIndexOutOfBoundsException", "an offset outside the text");
        return this;
    }
    uint32_t tail = text->count - at;
    add_units(text, text_of(added)->units, count);
    memmove(text->units + at + count, text->units + at, tail * sizeof *text->units);
    memcpy(text->units + at, text_of(added)->units, count * sizeof *text->units);
    return this;
}

static ref StringBuilder_reverse(ref this) {
    struct text *text = text_of(this);
    for (uint32_t n = 0; n < text->count / 2; n++) {
        uint16_t unit = text->units[n];
        text->units[n] = text->units[text->count - 1 - n];
        text->units[text->count - 1 - n] = unit;
    }
    return this;
}

static void StringBuilder_setLength(ref this, uint32_t length) {
    struct text *text = text_of(this);
    while (text->count < length) {
        uint16_t zero = 0;
        add_units(text, &zero, 1);
    }
    text->count = length;
}

// The text of a string as bytes, for the C library to read a number from; what is no ASCII is `?`.
static void narrow(ref string, char *out, size_t size) {
    size_t n = 0;
    for (; string && n + 1 < size && n < text_of(string)->count; n++) out[n] = text_of(string)->units[n] < 0x80 ? (char)text_of(string)->units[n] : '?';
    out[n] = 0;
}

static void bad_number(const char *text) {
    char message[120];
    snprintf(message, sizeof message, "For input string: \"%s\"", text);
    throw_new("java/lang/NumberFormatException", message);
}

uint64_t java_lang_Long__parseLong__Ljava_lang_String_2_J(ref string) {
    char text[64], *end;
    narrow(string, text, sizeof text);
    long long value = strtoll(text, &end, 10);
    if (!text[0] || *end || text[0] == ' ') return bad_number(text), 0;
    return (uint64_t)value;
}

uint32_t java_lang_Integer__parseInt__Ljava_lang_String_2_I(ref string) {
    char text[64];
    narrow(string, text, sizeof text);
    int64_t value = (int64_t)java_lang_Long__parseLong__Ljava_lang_String_2_J(string);
    if (!pending && (value > INT32_MAX || value < INT32_MIN)) return bad_number(text), 0;
    return (uint32_t)value;
}

double java_lang_Double__parseDouble__Ljava_lang_String_2_D(ref string) {
    char text[80], *end;
    narrow(string, text, sizeof text);
    double value = strtod(text, &end);
    if (end == text || *end) return bad_number(text), 0;
    return value;
}

uint32_t java_lang_Boolean__parseBoolean__Ljava_lang_String_2_Z(ref string) {
    char text[8];
    narrow(string, text, sizeof text);
    for (char *c = text; *c; c++) *c = (char)tolower((unsigned char)*c);
    return strcmp(text, "true") == 0;
}

ref java_lang_Integer__toString__I_Ljava_lang_String_2(uint32_t v) {
    return java_lang_String__valueOf__I_Ljava_lang_String_2(v);
}

ref java_lang_Integer__toHexString__I_Ljava_lang_String_2(uint32_t v) {
    char digits[16];
    snprintf(digits, sizeof digits, "%x", v);
    return string_ascii(digits);
}

ref java_lang_Long__toString__J_Ljava_lang_String_2(uint64_t v) {
    char digits[24];
    snprintf(digits, sizeof digits, "%lld", (long long)v);
    return string_ascii(digits);
}

uint32_t java_lang_Integer__compare__II_I(uint32_t a, uint32_t b) {
    return (uint32_t)(((int32_t)a > (int32_t)b) - ((int32_t)a < (int32_t)b));
}

uint32_t java_lang_Character__isDigit__C_Z(uint32_t c) { return iswdigit((wint_t)c) != 0; }
uint32_t java_lang_Character__isLetter__C_Z(uint32_t c) { return iswalpha((wint_t)c) != 0; }
uint32_t java_lang_Character__isWhitespace__C_Z(uint32_t c) { return iswspace((wint_t)c) != 0 && c != 0xa0; }
uint32_t java_lang_Character__isUpperCase__C_Z(uint32_t c) { return iswupper((wint_t)c) != 0; }
uint32_t java_lang_Character__isLowerCase__C_Z(uint32_t c) { return iswlower((wint_t)c) != 0; }
uint32_t java_lang_Character__toUpperCase__C_C(uint32_t c) { return upper((uint16_t)c); }
uint32_t java_lang_Character__toLowerCase__C_C(uint32_t c) { return lower((uint16_t)c); }

uint32_t java_lang_Character__forDigit__II_C(uint32_t digit, uint32_t radix) {
    if (radix < 2 || radix > 36 || digit >= radix) return 0;
    return digit < 10 ? '0' + digit : 'a' + digit - 10;
}

double java_lang_Math__abs__D_D(double v) { return fabs(v); }
double java_lang_Math__sqrt__D_D(double v) { return sqrt(v); }
double java_lang_Math__pow__DD_D(double a, double b) { return pow(a, b); }
double java_lang_Math__floor__D_D(double v) { return floor(v); }
double java_lang_Math__ceil__D_D(double v) { return ceil(v); }
uint64_t java_lang_Math__round__D_J(double v) { return jvm_d2l(floor(v + 0.5)); }
uint32_t java_lang_Math__max__II_I(uint32_t a, uint32_t b) { return (int32_t)a > (int32_t)b ? a : b; }
uint32_t java_lang_Math__min__II_I(uint32_t a, uint32_t b) { return (int32_t)a < (int32_t)b ? a : b; }
uint64_t java_lang_Math__max__JJ_J(uint64_t a, uint64_t b) { return (int64_t)a > (int64_t)b ? a : b; }
uint64_t java_lang_Math__min__JJ_J(uint64_t a, uint64_t b) { return (int64_t)a < (int64_t)b ? a : b; }

void java_lang_System__arraycopy__Ljava_lang_Object_2ILjava_lang_Object_2II_V(ref from, uint32_t from_at, ref to, uint32_t to_at, uint32_t count) {
    if (!from || !to) {
        jvm_null();
        return;
    }
    int32_t have = LENGTH(from), space = LENGTH(to);
    if ((int32_t)from_at < 0 || (int32_t)to_at < 0 || (int32_t)count < 0 || (int32_t)from_at > have - (int32_t)count || (int32_t)to_at > space - (int32_t)count) {
        throw_new("java/lang/ArrayIndexOutOfBoundsException", "arraycopy: out of bounds");
        return;
    }
    uint64_t size = element_size(class_of(from));
    memmove(to + ELEMENTS + size * to_at, from + ELEMENTS + size * from_at, size * count);
}

static void list_room(struct list *list) {
    if (list->count == list->room) {
        list->room = list->room * 2 + 8;
        list->items = realloc(list->items, list->room * sizeof *list->items);
        if (!list->items) stop("out of memory for", "a list");
    }
}

static int32_t list_find(struct list *list, ref item) {
    for (uint32_t n = 0; n < list->count; n++) {
        if (java_util_Objects__equals__Ljava_lang_Object_2Ljava_lang_Object_2_Z(item, list->items[n])) return (int32_t)n;
    }
    return -1;
}

static void list_bounds(uint32_t index, uint32_t count) {
    char message[80];
    snprintf(message, sizeof message, "Index %d out of bounds for length %u", (int32_t)index, count);
    throw_new("java/lang/IndexOutOfBoundsException", message);
}

static void ArrayList_addAt(ref this, uint32_t index, ref item) {
    struct list *list = list_of(this);
    if (index > list->count) {
        list_bounds(index, list->count);
        return;
    }
    list_room(list);
    memmove(list->items + index + 1, list->items + index, (list->count - index) * sizeof *list->items);
    list->items[index] = item;
    list->count++;
}

static ref ArrayList_removeAt(ref this, uint32_t index) {
    struct list *list = list_of(this);
    if (index >= list->count) {
        list_bounds(index, list->count);
        return 0;
    }
    ref removed = list->items[index];
    memmove(list->items + index, list->items + index + 1, (list->count - index - 1) * sizeof *list->items);
    list->count--;
    return removed;
}

static uint32_t ArrayList_remove(ref this, ref item) {
    int32_t at = list_find(list_of(this), item);
    if (at >= 0) ArrayList_removeAt(this, (uint32_t)at);
    return at >= 0;
}

static uint32_t ArrayList_indexOf(ref this, ref item) {
    return (uint32_t)list_find(list_of(this), item);
}

static ref ArrayList_set(ref this, uint32_t index, ref item) {
    struct list *list = list_of(this);
    if (index >= list->count) {
        list_bounds(index, list->count);
        return 0;
    }
    ref old = list->items[index];
    list->items[index] = item;
    return old;
}

static void ArrayList_clear(ref this) {
    list_of(this)->count = 0;
}

// A map is two lists side by side: its keys in the order they came, and each one's value.
struct map {
    struct list keys, values;
    // The most entries the map has held, which says how many places its table has.
    uint32_t peak;
};

static struct map *map_of(ref object) {
    return (struct map *)(uintptr_t)NATIVE(object);
}

void java_util_HashMap__new___V(ref this) {
    (void)this;
}

static uint32_t HashMap_size(ref this) {
    return map_of(this)->keys.count;
}

static uint32_t HashMap_isEmpty(ref this) {
    return map_of(this)->keys.count == 0;
}

static uint32_t HashMap_containsKey(ref this, ref key) {
    return list_find(&map_of(this)->keys, key) >= 0;
}

static ref HashMap_getOrDefault(ref this, ref key, ref otherwise) {
    int32_t at = list_find(&map_of(this)->keys, key);
    return at >= 0 ? map_of(this)->values.items[at] : otherwise;
}

static ref HashMap_get(ref this, ref key) {
    return HashMap_getOrDefault(this, key, 0);
}

static ref HashMap_put(ref this, ref key, ref value) {
    struct map *map = map_of(this);
    int32_t at = list_find(&map->keys, key);
    if (at >= 0) {
        ref old = map->values.items[at];
        map->values.items[at] = value;
        return old;
    }
    list_room(&map->keys);
    list_room(&map->values);
    map->keys.items[map->keys.count++] = key;
    map->values.items[map->values.count++] = value;
    if (map->keys.count > map->peak) map->peak = map->keys.count;
    return 0;
}

static ref HashMap_remove(ref this, ref key) {
    struct map *map = map_of(this);
    int32_t at = list_find(&map->keys, key);
    if (at < 0) return 0;
    ref old = map->values.items[at];
    for (struct list *list = &map->keys; list; list = list == &map->keys ? &map->values : 0) {
        memmove(list->items + at, list->items + at + 1, (list->count - (uint32_t)at - 1) * sizeof *list->items);
        list->count--;
    }
    return old;
}

static void HashMap_clear(ref this) {
    map_of(this)->keys.count = 0;
    map_of(this)->values.count = 0;
}

// A map's keys (0), its values (1) or its entries (2) as a list, in the order a `HashMap` walks
// them: by the place of each key's hash in a table as large as the most the map has held asks
// for, and within a place by the order the keys came. The list is of what the map holds now.
static ref walked(ref this, int what) {
    struct map *map = map_of(this);
    uint32_t count = map->keys.count, places = 16;
    while (map->peak > places / 4 * 3) places *= 2;
    uint32_t *place = room(count * sizeof *place + 1), *order = room(count * sizeof *order + 1);
    for (uint32_t n = 0; n < count; n++) {
        ref key = map->keys.items[n];
        uint32_t hash = key ? ((uint32_t (*)(ref))jvm_virtual(key, (uint8_t *)"hashCode:()I"))(key) : 0;
        place[n] = (hash ^ (hash >> 16)) & (places - 1);
        // Into its place among those before it; one of the same place stays ahead.
        uint32_t at = n;
        for (; at > 0 && place[order[at - 1]] > place[n]; at--) order[at] = order[at - 1];
        order[at] = n;
    }
    ref made = object("java/util/ArrayList");
    for (uint32_t n = 0; n < count; n++) {
        ref key = map->keys.items[order[n]], value = map->values.items[order[n]];
        if (what == 2) {
            ref node = allocate("java/util/HashMap$Node", 8);
            ref *pair = room(2 * sizeof *pair);
            pair[0] = key;
            pair[1] = value;
            NATIVE(node) = (uint64_t)(uintptr_t)pair;
            ArrayList_add(made, node);
        } else {
            ArrayList_add(made, what ? value : key);
        }
    }
    free(place);
    free(order);
    return made;
}

static ref HashMap_keySet(ref this) { return walked(this, 0); }
static ref HashMap_values(ref this) { return walked(this, 1); }
static ref HashMap_entrySet(ref this) { return walked(this, 2); }
static ref Node_getKey(ref this) { return ((ref *)(uintptr_t)NATIVE(this))[0]; }
static ref Node_getValue(ref this) { return ((ref *)(uintptr_t)NATIVE(this))[1]; }

void java_util_HashSet__new___V(ref this) {
    (void)this;
}

static uint32_t HashSet_add(ref this, ref item) {
    if (list_find(&map_of(this)->keys, item) >= 0) return 0;
    HashMap_put(this, item, item);
    return 1;
}

static uint32_t HashSet_remove(ref this, ref item) {
    if (list_find(&map_of(this)->keys, item) < 0) return 0;
    HashMap_remove(this, item);
    return 1;
}

static ref HashSet_iterator(ref this) {
    return ArrayList_iterator(walked(this, 0));
}

// The elements of a collection of the library as a list: a list's own, or a set's as it walks them.
static ref items_of(ref collection) {
    if (!collection) {
        throw_new("java/lang/NullPointerException", 0);
        return 0;
    }
    return is(class_of(collection), "java/util/HashSet") ? walked(collection, 0) : collection;
}

static uint32_t ArrayList_addAll(ref this, ref from) {
    ref items = items_of(from);
    if (!items) return 0;
    uint32_t count = list_of(items)->count;
    for (uint32_t n = 0; n < count; n++) ArrayList_add(this, list_of(items)->items[n]);
    return count != 0;
}

void java_util_ArrayList__new__Ljava_util_Collection_2_V(ref this, ref from) {
    ArrayList_addAll(this, from);
}

ref java_util_Arrays__asList___3Ljava_lang_Object_2_Ljava_util_List_2(ref array) {
    ref made = object("java/util/ArrayList");
    for (int32_t n = 0; array && n < LENGTH(array); n++) ArrayList_add(made, ((ref *)(array + ELEMENTS))[n]);
    return made;
}

// A list sorted by its elements' own order, or by a comparator's; equal elements keep their places.
static void ArrayList_sort(ref this, ref comparator) {
    struct list *list = list_of(this);
    for (uint32_t sorted = 1; sorted < list->count; sorted++) {
        for (uint32_t at = sorted; at > 0; at--) {
            ref a = list->items[at - 1], b = list->items[at];
            int32_t order;
            if (comparator) {
                order = (int32_t)((uint32_t (*)(ref, ref, ref))jvm_virtual(comparator, (uint8_t *)"compare:(Ljava/lang/Object;Ljava/lang/Object;)I"))(comparator, a, b);
            } else {
                order = (int32_t)((uint32_t (*)(ref, ref))jvm_virtual(a, (uint8_t *)"compareTo:(Ljava/lang/Object;)I"))(a, b);
            }
            if (pending) return;
            if (order <= 0) break;
            list->items[at - 1] = b;
            list->items[at] = a;
        }
    }
}

void java_util_Collections__sort__Ljava_util_List_2_V(ref list) {
    if (!list) {
        throw_new("java/lang/NullPointerException", 0);
        return;
    }
    ArrayList_sort(list, 0);
}

static uint32_t ArrayList_removeIf(ref this, ref predicate) {
    struct list *list = list_of(this);
    uint32_t kept = 0, count = list->count;
    for (uint32_t n = 0; n < count; n++) {
        ref item = list->items[n];
        uint32_t gone = ((uint32_t (*)(ref, ref))jvm_virtual(predicate, (uint8_t *)"test:(Ljava/lang/Object;)Z"))(predicate, item);
        if (pending) return 0;
        if (!(gone & 1)) list->items[kept++] = item;
    }
    list->count = kept;
    return kept != count;
}

static uint32_t Integer_compareTo(ref this, ref other) {
    int32_t a = (int32_t)NATIVE(this), b = (int32_t)NATIVE(other);
    return (uint32_t)((a > b) - (a < b));
}

static uint32_t Long_compareTo(ref this, ref other) {
    int64_t a = (int64_t)NATIVE(this), b = (int64_t)NATIVE(other);
    return (uint32_t)((a > b) - (a < b));
}

static void everyday(void);

static void library(const char *name, const char *above, const char *first, const char *second) {
    define(name, above, 0);
    if (first) jvm_implement((uint8_t *)name, (uint8_t *)first);
    if (second) jvm_implement((uint8_t *)name, (uint8_t *)second);
}

uint32_t java_lang_Integer__sum__II_I(uint32_t a, uint32_t b) { return a + b; }
uint32_t java_lang_Integer__max__II_I(uint32_t a, uint32_t b) { return (int32_t)a > (int32_t)b ? a : b; }
uint32_t java_lang_Integer__min__II_I(uint32_t a, uint32_t b) { return (int32_t)a < (int32_t)b ? a : b; }
uint32_t java_lang_Integer__signum__I_I(uint32_t v) { return (uint32_t)(((int32_t)v > 0) - ((int32_t)v < 0)); }

uint32_t java_lang_Integer__bitCount__I_I(uint32_t v) {
    uint32_t count = 0;
    for (; v; v &= v - 1) count++;
    return count;
}

// As `Double.compare` orders them: what is no number is above every number, and zero above its
// negative.
uint32_t java_lang_Double__compare__DD_I(double a, double b) {
    if (a < b) return (uint32_t)-1;
    if (a > b) return 1;
    if (a != a || b != b) return (uint32_t)((a != a) - (b != b));
    int64_t x, y;
    memcpy(&x, &a, sizeof x);
    memcpy(&y, &b, sizeof y);
    return (uint32_t)((x > y) - (x < y));
}

static int32_t floor_quotient(int32_t a, int32_t b) {
    if (b == 0) {
        throw_new("java/lang/ArithmeticException", "/ by zero");
        return 0;
    }
    if (b == -1) return (int32_t)(0u - (uint32_t)a);
    int32_t quotient = a / b;
    return a % b != 0 && ((a < 0) != (b < 0)) ? quotient - 1 : quotient;
}

uint32_t java_lang_Math__floorDiv__II_I(uint32_t a, uint32_t b) { return (uint32_t)floor_quotient((int32_t)a, (int32_t)b); }

uint32_t java_lang_Math__floorMod__II_I(uint32_t a, uint32_t b) {
    uint32_t quotient = (uint32_t)floor_quotient((int32_t)a, (int32_t)b);
    return pending ? 0 : a - quotient * b;
}

uint32_t java_lang_Integer__parseInt__Ljava_lang_String_2I_I(ref string, uint32_t radix) {
    char text[64], *end;
    narrow(string, text, sizeof text);
    long long value = radix >= 2 && radix <= 36 ? strtoll(text, &end, (int)radix) : 0;
    // The C library reads what the Java one refuses: a space before the number, a `0x`.
    int plain = text[0] && (isalnum((unsigned char)text[0]) || ((text[0] == '-' || text[0] == '+') && isalnum((unsigned char)text[1])));
    if (radix == 16 && (text[0] == '0' || text[1] == '0') && strpbrk(text, "xX")) plain = 0;
    if (radix < 2 || radix > 36 || !plain || *end || value > INT32_MAX || value < INT32_MIN) {
        char message[120];
        if (radix == 10) {
            snprintf(message, sizeof message, "For input string: \"%s\"", text);
        } else {
            snprintf(message, sizeof message, "For input string: \"%s\" under radix %d", text, (int)radix);
        }
        throw_new("java/lang/NumberFormatException", message);
        return 0;
    }
    return (uint32_t)value;
}

// A digit's value, or a Latin letter's as a digit of a higher base; else -1.
uint32_t java_lang_Character__getNumericValue__C_I(uint32_t c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'z') return c - 'a' + 10;
    if (c >= 'A' && c <= 'Z') return c - 'A' + 10;
    return (uint32_t)-1;
}

uint32_t java_lang_Character__isLetterOrDigit__C_Z(uint32_t c) { return iswalpha((wint_t)c) != 0 || iswdigit((wint_t)c) != 0; }

uint32_t java_util_Objects__hash___3Ljava_lang_Object_2_I(ref objects) {
    uint32_t hash = 1;
    for (int32_t n = 0; objects && n < LENGTH(objects); n++) {
        ref item = ((ref *)(objects + ELEMENTS))[n];
        uint32_t of = item ? ((uint32_t (*)(ref))jvm_virtual(item, (uint8_t *)"hashCode:()I"))(item) : 0;
        hash = hash * 31 + of;
    }
    return hash;
}

ref java_util_Objects__toString__Ljava_lang_Object_2_Ljava_lang_String_2(ref object) { return describe(object); }

static uint32_t String_equalsIgnoreCase(ref this, ref other) {
    if (!other || text_of(this)->count != text_of(other)->count) return 0;
    for (uint32_t n = 0; n < text_of(this)->count; n++) {
        if (lower(upper(text_of(this)->units[n])) != lower(upper(text_of(other)->units[n]))) return 0;
    }
    return 1;
}

static ref String_replaceText(ref this, ref from, ref to) {
    struct text *text = text_of(this), *old = text_of(describe(from)), *new = text_of(describe(to));
    ref made = object("java/lang/String");
    for (uint32_t at = 0; at < text->count;) {
        if (old->count && at + old->count <= text->count && memcmp(text->units + at, old->units, old->count * sizeof *old->units) == 0) {
            add_units(text_of(made), new->units, new->count);
            at += old->count;
        } else {
            add_units(text_of(made), text->units + at, 1);
            at++;
        }
    }
    return made;
}

static ref StringBuilder_delete(ref this, uint32_t from, uint32_t to) {
    struct text *text = text_of(this);
    if ((int32_t)from < 0 || from > text->count || (int32_t)from > (int32_t)to) {
        throw_new("java/lang/StringIndexOutOfBoundsException", "a run outside the text");
        return this;
    }
    if (to > text->count) to = text->count;
    memmove(text->units + from, text->units + to, (text->count - to) * sizeof *text->units);
    text->count -= to - from;
    return this;
}

static ref StringBuilder_deleteCharAt(ref this, uint32_t at) {
    if (at >= text_of(this)->count) {
        throw_new("java/lang/StringIndexOutOfBoundsException", "an index outside the text");
        return this;
    }
    return StringBuilder_delete(this, at, at + 1);
}

static void StringBuilder_setCharAt(ref this, uint32_t at, uint32_t unit) {
    if (at >= text_of(this)->count) {
        throw_new("java/lang/StringIndexOutOfBoundsException", "an index outside the text");
        return;
    }
    text_of(this)->units[at] = (uint16_t)unit;
}

static uint32_t Class_desiredAssertionStatus(ref this) {
    (void)this;
    return 0;
}

// An object's class, and a class's names: the whole one with its package, and the simple one,
// which is what follows the package and the classes it is nested in.
static ref Object_getClass(ref this) {
    return jvm_class((uint8_t *)class_of(this));
}

static ref Class_getName(ref this) {
    char name[256];
    dotted(name, sizeof name, (const char *)(uintptr_t)NATIVE(this));
    return string_ascii(name);
}

static ref Class_getSimpleName(ref this) {
    const char *name = (const char *)(uintptr_t)NATIVE(this), *simple = name;
    for (const char *c = name; *c; c++) {
        if (*c == '/' || *c == '$') simple = c + 1;
    }
    return string_ascii(simple);
}

// A number in a base that is a power of two, without a sign.
static ref radix_text(uint64_t v, unsigned bits) {
    char digits[65];
    int at = 64;
    digits[at] = 0;
    do {
        digits[--at] = "0123456789abcdef"[v & ((1u << bits) - 1)];
        v >>= bits;
    } while (v);
    return string_ascii(digits + at);
}

ref java_lang_Long__toHexString__J_Ljava_lang_String_2(uint64_t v) { return radix_text(v, 4); }
ref java_lang_Long__toBinaryString__J_Ljava_lang_String_2(uint64_t v) { return radix_text(v, 1); }
ref java_lang_Integer__toBinaryString__I_Ljava_lang_String_2(uint32_t v) { return radix_text(v, 1); }
ref java_lang_Integer__toOctalString__I_Ljava_lang_String_2(uint32_t v) { return radix_text(v, 3); }

// `Arrays.sort` of an array of numbers: a character has no sign, the rest have.
#define SORTS(letter, type) \
    static int compare_##letter(const void *a, const void *b) { \
        type x = *(const type *)a, y = *(const type *)b; \
        return (x > y) - (x < y); \
    } \
    void java_util_Arrays__sort___3##letter##_V(ref sorted) { \
        if (!sorted) { \
            throw_new("java/lang/NullPointerException", 0); \
            return; \
        } \
        qsort(sorted + ELEMENTS, (size_t)LENGTH(sorted), sizeof(type), compare_##letter); \
    }
SORTS(I, int32_t)
SORTS(J, int64_t)
SORTS(C, uint16_t)
SORTS(S, int16_t)
SORTS(B, int8_t)

// `split` by a separator that is literal text: the pieces between its places, without the empty
// ones at the end. A pattern is not read.
static ref String_split(ref this, ref separator) {
    struct text *text = text_of(this), *part = text_of(separator);
    int literal = part->count != 0;
    for (uint32_t n = 0; n < part->count; n++) {
        if (part->units[n] < 0x80 && strchr("\\^$.|?*+()[]{}", (int)part->units[n])) literal = 0;
    }
    if (!literal) {
        throw_new("java/lang/UnsupportedOperationException", "a pattern that is not literal text");
        return 0;
    }
    // Twice over the text: the pieces counted, then made.
    ref made = 0;
    for (int pass = 0; pass < 2; pass++) {
        uint32_t pieces = 0, kept = 0, from = 0, at = 0;
        int found = 0;
        while (at + part->count <= text->count) {
            if (memcmp(text->units + at, part->units, part->count * sizeof *part->units) == 0) {
                if (made && (int32_t)pieces < LENGTH(made)) ((ref *)(made + ELEMENTS))[pieces] = string_of(text->units + from, at - from);
                pieces++;
                if (at > from) kept = pieces;
                at += part->count;
                from = at;
                found = 1;
            } else {
                at++;
            }
        }
        if (made && (int32_t)pieces < LENGTH(made)) ((ref *)(made + ELEMENTS))[pieces] = string_of(text->units + from, text->count - from);
        pieces++;
        if (text->count > from) kept = pieces;
        if (!made) {
            made = array("[Ljava/lang/String;", (int32_t)(found ? kept : 1));
            if (!made) return 0;
        }
    }
    return made;
}

// The shortest digits that name a double that is not negative, and its decimal exponent: the
// value is the first digit, a point, the rest, times ten to the exponent.
static void shortest(double v, char *digits, int *exponent) {
    char written[40];
    for (int precision = 0; precision < 17; precision++) {
        snprintf(written, sizeof written, "%.*e", precision, v);
        if (strtod(written, 0) == v) break;
    }
    char *at = written;
    for (; *at && *at != 'e'; at++) {
        if (*at >= '0' && *at <= '9') *digits++ = *at;
    }
    *digits = 0;
    *exponent = *at ? atoi(at + 1) : 0;
}

// A double that is not negative as `%.Nf` writes it: its shortest digits rounded half up at the
// precision, which is how the library rounds (1.005 to two places is 1.01, though the double
// lies below it).
static void fixed_text(double v, int precision, struct text *out) {
    if (v != v || isinf(v)) {
        add_ascii(out, v != v ? "NaN" : "Infinity");
        return;
    }
    char digits[24];
    int exponent;
    shortest(v, digits, &exponent);
    if (precision > 340) precision = 340;
    // The digits before the point, then those after it, with room for one more to round by.
    static char all[720];
    int count = 0, whole;
    if (exponent < 0) {
        all[count++] = '0';
        for (int n = 0; n < -exponent - 1 && count < 700; n++) all[count++] = '0';
        for (char *d = digits; *d && count < 700; d++) all[count++] = *d;
        whole = 1;
    } else {
        for (char *d = digits; *d; d++) all[count++] = *d;
        whole = exponent + 1;
        while (count < whole) all[count++] = '0';
    }
    int up = whole + precision < count && all[whole + precision] >= '5';
    while (count < whole + precision) all[count++] = '0';
    count = whole + precision;
    int carried = 0;
    if (up) {
        int at = count;
        for (;;) {
            if (at == 0) {
                carried = 1;
                break;
            }
            at--;
            if (all[at] == '9') {
                all[at] = '0';
            } else {
                all[at]++;
                break;
            }
        }
    }
    all[count] = 0;
    if (carried) add_ascii(out, "1");
    uint16_t point = '.';
    for (int n = 0; n < count; n++) {
        uint16_t unit = (uint8_t)all[n];
        if (n == whole) add_units(out, &point, 1);
        add_units(out, &unit, 1);
    }
}

static void pad_with(struct text *out, char with, int count) {
    uint16_t unit = (uint8_t)with;
    for (; count > 0; count--) add_units(out, &unit, 1);
}

// `String.format`: the conversions `d`, `x`, `s`, `c`, `b`, `f`, `n` and `%`, with the flags
// `-`, `0` and `+`, a width and a precision. Another format is refused.
ref java_lang_String__format__Ljava_lang_String_2_3Ljava_lang_Object_2_Ljava_lang_String_2(ref format, ref arguments) {
    struct text *from = text_of(format);
    ref made = object("java/lang/String");
    struct text *out = text_of(made);
    int32_t next = 0, count = arguments ? LENGTH(arguments) : 0;
    for (uint32_t n = 0; n < from->count; n++) {
        if (from->units[n] != '%') {
            add_units(out, &from->units[n], 1);
            continue;
        }
        int left = 0, zero = 0, plus = 0, width = 0, precision = -1;
        for (n++; n < from->count && (from->units[n] == '-' || from->units[n] == '0' || from->units[n] == '+'); n++) {
            left |= from->units[n] == '-';
            zero |= from->units[n] == '0';
            plus |= from->units[n] == '+';
        }
        for (; n < from->count && from->units[n] >= '0' && from->units[n] <= '9'; n++) width = width * 10 + (from->units[n] - '0');
        if (n < from->count && from->units[n] == '.') {
            precision = 0;
            for (n++; n < from->count && from->units[n] >= '0' && from->units[n] <= '9'; n++) precision = precision * 10 + (from->units[n] - '0');
        }
        if (n >= from->count) {
            throw_new("java/lang/UnsupportedOperationException", "a format that ends in its conversion");
            return 0;
        }
        uint16_t conversion = from->units[n];
        if (conversion == 'n' || conversion == '%') {
            pad_with(out, conversion == 'n' ? '\n' : '%', 1);
            continue;
        }
        if (next >= count) {
            throw_new("java/lang/UnsupportedOperationException", "a format with no argument for its conversion");
            return 0;
        }
        ref argument = ((ref *)(arguments + ELEMENTS))[next++];
        const char *class = argument ? class_of(argument) : "";
        uint64_t bits = argument ? NATIVE(argument) : 0;
        int is_long = strcmp(class, "java/lang/Long") == 0, is_boolean = strcmp(class, "java/lang/Boolean") == 0;
        int is_whole = is_long || strcmp(class, "java/lang/Integer") == 0;
        const char *sign = "";
        // The conversion's text, made apart so the width can be filled round it.
        struct text body = {0};
        char digits[24];
        if (conversion == 's') {
            struct text *described = text_of(describe(argument));
            if (pending) return 0;
            uint32_t kept = precision >= 0 && (uint32_t)precision < described->count ? (uint32_t)precision : described->count;
            add_units(&body, described->units, kept);
        } else if (conversion == 'b') {
            add_ascii(&body, (is_boolean ? (bits & 1) != 0 : argument != 0) ? "true" : "false");
        } else if (conversion == 'c' && strcmp(class, "java/lang/Character") == 0) {
            uint16_t unit = (uint16_t)bits;
            add_units(&body, &unit, 1);
        } else if (conversion == 'd' && (is_whole || strcmp(class, "java/lang/Short") == 0 || strcmp(class, "java/lang/Byte") == 0)) {
            int64_t value = is_long ? (int64_t)bits : (int64_t)(int32_t)(uint32_t)bits;
            sign = value < 0 ? "-" : plus ? "+" : "";
            snprintf(digits, sizeof digits, "%llu", value < 0 ? 0ull - (unsigned long long)value : (unsigned long long)value);
            add_ascii(&body, digits);
        } else if (conversion == 'x' && is_whole) {
            snprintf(digits, sizeof digits, "%llx", (unsigned long long)(is_long ? bits : bits & 0xffffffffu));
            add_ascii(&body, digits);
        } else if (conversion == 'f' && (strcmp(class, "java/lang/Double") == 0 || strcmp(class, "java/lang/Float") == 0)) {
            double value;
            if (strcmp(class, "java/lang/Float") == 0) {
                uint32_t narrow = (uint32_t)bits;
                float single;
                memcpy(&single, &narrow, sizeof single);
                value = single;
            } else {
                memcpy(&value, &bits, sizeof value);
            }
            sign = signbit(value) && value == value ? "-" : plus ? "+" : "";
            fixed_text(fabs(value), precision < 0 ? 6 : precision, &body);
            zero = zero && value == value && !isinf(value);
        } else {
            throw_new("java/lang/UnsupportedOperationException", "a conversion of a format the runtime has not");
            return 0;
        }
        int pad = width - (int)strlen(sign) - (int)body.count;
        int numeric = conversion == 'd' || conversion == 'x' || conversion == 'f';
        if (!left && !(zero && numeric)) pad_with(out, ' ', pad);
        add_ascii(out, sign);
        if (!left && zero && numeric) pad_with(out, '0', pad);
        add_units(out, body.units, body.count);
        if (left) pad_with(out, ' ', pad);
        free(body.units);
    }
    return made;
}

static void method(const char *name, const char *named, void *function) {
    jvm_method((uint8_t *)name, (uint8_t *)named, function);
}

// The methods of the everyday library an object's class finds.
static void everyday(void) {
    const char *string = "java/lang/String", *builder = "java/lang/StringBuilder", *list = "java/util/ArrayList", *map = "java/util/HashMap";
    method(string, "indexOf:(I)I", (void *)String_indexOfChar);
    method(string, "lastIndexOf:(I)I", (void *)String_lastIndexOfChar);
    method(string, "indexOf:(Ljava/lang/String;)I", (void *)String_indexOf);
    method(string, "contains:(Ljava/lang/CharSequence;)Z", (void *)String_contains);
    method(string, "startsWith:(Ljava/lang/String;)Z", (void *)String_startsWith);
    method(string, "endsWith:(Ljava/lang/String;)Z", (void *)String_endsWith);
    method(string, "replace:(CC)Ljava/lang/String;", (void *)String_replace);
    method(string, "toUpperCase:()Ljava/lang/String;", (void *)String_toUpperCase);
    method(string, "toLowerCase:()Ljava/lang/String;", (void *)String_toLowerCase);
    method(string, "trim:()Ljava/lang/String;", (void *)String_trim);
    method(string, "substring:(I)Ljava/lang/String;", (void *)String_substring);
    method(string, "substring:(II)Ljava/lang/String;", (void *)String_substring2);
    method(string, "compareTo:(Ljava/lang/String;)I", (void *)String_compareTo);
    method(string, "compareTo:(Ljava/lang/Object;)I", (void *)String_compareTo);
    method(string, "toCharArray:()[C", (void *)String_toCharArray);
    method(string, "split:(Ljava/lang/String;)[Ljava/lang/String;", (void *)String_split);
    method(string, "equalsIgnoreCase:(Ljava/lang/String;)Z", (void *)String_equalsIgnoreCase);
    method(string, "replace:(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Ljava/lang/String;", (void *)String_replaceText);
    method(builder, "delete:(II)Ljava/lang/StringBuilder;", (void *)StringBuilder_delete);
    method(builder, "deleteCharAt:(I)Ljava/lang/StringBuilder;", (void *)StringBuilder_deleteCharAt);
    method(builder, "setCharAt:(IC)V", (void *)StringBuilder_setCharAt);
    method("java/lang/Class", "desiredAssertionStatus:()Z", (void *)Class_desiredAssertionStatus);
    method("java/lang/Object", "getClass:()Ljava/lang/Class;", (void *)Object_getClass);
    method("java/lang/Class", "getName:()Ljava/lang/String;", (void *)Class_getName);
    method("java/lang/Class", "getSimpleName:()Ljava/lang/String;", (void *)Class_getSimpleName);
    method(builder, "insert:(ILjava/lang/String;)Ljava/lang/StringBuilder;", (void *)StringBuilder_insert);
    method(builder, "reverse:()Ljava/lang/StringBuilder;", (void *)StringBuilder_reverse);
    method(builder, "setLength:(I)V", (void *)StringBuilder_setLength);
    method(list, "add:(ILjava/lang/Object;)V", (void *)ArrayList_addAt);
    method(list, "remove:(I)Ljava/lang/Object;", (void *)ArrayList_removeAt);
    method(list, "remove:(Ljava/lang/Object;)Z", (void *)ArrayList_remove);
    method(list, "indexOf:(Ljava/lang/Object;)I", (void *)ArrayList_indexOf);
    method(list, "set:(ILjava/lang/Object;)Ljava/lang/Object;", (void *)ArrayList_set);
    method(list, "clear:()V", (void *)ArrayList_clear);
    library(map, "java/lang/Object", "java/util/Map", 0);
    method(map, "size:()I", (void *)HashMap_size);
    method(map, "isEmpty:()Z", (void *)HashMap_isEmpty);
    method(map, "containsKey:(Ljava/lang/Object;)Z", (void *)HashMap_containsKey);
    method(map, "get:(Ljava/lang/Object;)Ljava/lang/Object;", (void *)HashMap_get);
    method(map, "getOrDefault:(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;", (void *)HashMap_getOrDefault);
    method(map, "put:(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;", (void *)HashMap_put);
    method(map, "remove:(Ljava/lang/Object;)Ljava/lang/Object;", (void *)HashMap_remove);
    method(map, "clear:()V", (void *)HashMap_clear);
    method(map, "keySet:()Ljava/util/Set;", (void *)HashMap_keySet);
    method(map, "values:()Ljava/util/Collection;", (void *)HashMap_values);
    method(map, "entrySet:()Ljava/util/Set;", (void *)HashMap_entrySet);
    const char *node = "java/util/HashMap$Node", *set = "java/util/HashSet";
    library(node, "java/lang/Object", "java/util/Map$Entry", 0);
    method(node, "getKey:()Ljava/lang/Object;", (void *)Node_getKey);
    method(node, "getValue:()Ljava/lang/Object;", (void *)Node_getValue);
    library(set, "java/lang/Object", "java/util/Set", "java/util/Collection");
    method(set, "size:()I", (void *)HashMap_size);
    method(set, "isEmpty:()Z", (void *)HashMap_isEmpty);
    method(set, "contains:(Ljava/lang/Object;)Z", (void *)HashMap_containsKey);
    method(set, "add:(Ljava/lang/Object;)Z", (void *)HashSet_add);
    method(set, "remove:(Ljava/lang/Object;)Z", (void *)HashSet_remove);
    method(set, "clear:()V", (void *)HashMap_clear);
    method(set, "iterator:()Ljava/util/Iterator;", (void *)HashSet_iterator);
    method(list, "addAll:(Ljava/util/Collection;)Z", (void *)ArrayList_addAll);
    method(list, "sort:(Ljava/util/Comparator;)V", (void *)ArrayList_sort);
    method(list, "removeIf:(Ljava/util/function/Predicate;)Z", (void *)ArrayList_removeIf);
    method("java/lang/Integer", "compareTo:(Ljava/lang/Integer;)I", (void *)Integer_compareTo);
    method("java/lang/Integer", "compareTo:(Ljava/lang/Object;)I", (void *)Integer_compareTo);
    method("java/lang/Long", "compareTo:(Ljava/lang/Long;)I", (void *)Long_compareTo);
    method("java/lang/Long", "compareTo:(Ljava/lang/Object;)I", (void *)Long_compareTo);
    library("java/lang/NumberFormatException", "java/lang/IllegalArgumentException", 0, 0);
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
    method("java/lang/Object", "clone:()Ljava/lang/Object;", (void *)Object_clone);
    library("java/lang/Record", "java/lang/Object", 0, 0);
    library("java/lang/Enum", "java/lang/Object", "java/lang/Comparable", 0);
    method("java/lang/Enum", "name:()Ljava/lang/String;", (void *)Enum_name);
    method("java/lang/Enum", "toString:()Ljava/lang/String;", (void *)Enum_name);
    method("java/lang/Enum", "ordinal:()I", (void *)Enum_ordinal);
    method("java/lang/Enum", "hashCode:()I", (void *)Enum_ordinal);
    method("java/lang/Enum", "compareTo:(Ljava/lang/Enum;)I", (void *)Enum_compareTo);
    method("java/lang/Enum", "compareTo:(Ljava/lang/Object;)I", (void *)Enum_compareTo);
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
    // A builder or a string as the text it is, which is what its `toString` gives.
    method(builder, "append:(Ljava/lang/CharSequence;)Ljava/lang/StringBuilder;", (void *)java_lang_StringBuilder__append__Ljava_lang_Object_2_Ljava_lang_StringBuilder_2);
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
    method("java/util/ArrayList", "contains:(Ljava/lang/Object;)Z", (void *)ArrayList_contains);
    method("java/util/ArrayList", "toString:()Ljava/lang/String;", (void *)ArrayList_toString);
    method("java/util/ArrayList", "iterator:()Ljava/util/Iterator;", (void *)ArrayList_iterator);
    library("java/util/ArrayList$Itr", "java/lang/Object", "java/util/Iterator", 0);
    method("java/util/ArrayList$Itr", "hasNext:()Z", (void *)Iterator_hasNext);
    method("java/util/ArrayList$Itr", "next:()Ljava/lang/Object;", (void *)Iterator_next);
    library("java/lang/Throwable", "java/lang/Object", 0, 0);
    method("java/lang/Throwable", "getMessage:()Ljava/lang/String;", (void *)Throwable_getMessage);
    method("java/lang/Throwable", "toString:()Ljava/lang/String;", (void *)Throwable_toString);
    library("java/lang/Error", "java/lang/Throwable", 0, 0);
    library("java/lang/Exception", "java/lang/Throwable", 0, 0);
    library("java/lang/RuntimeException", "java/lang/Exception", 0, 0);
    library("java/lang/IllegalStateException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/IllegalArgumentException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/MatchException", "java/lang/RuntimeException", 0, 0);
    library("java/lang/CloneNotSupportedException", "java/lang/Exception", 0, 0);
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
    method("java/lang/Long", "toString:()Ljava/lang/String;", (void *)Long_toString);
    library("java/lang/Double", "java/lang/Number", "java/lang/Comparable", 0);
    method("java/lang/Double", "doubleValue:()D", (void *)Double_doubleValue);
    method("java/lang/Double", "toString:()Ljava/lang/String;", (void *)Double_toString);
    library("java/lang/Boolean", "java/lang/Object", "java/lang/Comparable", 0);
    method("java/lang/Boolean", "booleanValue:()Z", (void *)Box_intValue);
    method("java/lang/Boolean", "toString:()Ljava/lang/String;", (void *)Boolean_toString);
    library("java/lang/Character", "java/lang/Object", "java/lang/Comparable", 0);
    method("java/lang/Character", "charValue:()C", (void *)Box_intValue);
    method("java/lang/Character", "toString:()Ljava/lang/String;", (void *)Character_toString);
    static const char *const boxes[] = {"java/lang/Integer", "java/lang/Long", "java/lang/Double", "java/lang/Boolean", "java/lang/Character"};
    for (int n = 0; n < 5; n++) {
        method(boxes[n], "equals:(Ljava/lang/Object;)Z", (void *)Box_equals);
        method(boxes[n], "hashCode:()I", (void *)Box_hashCode);
    }
    everyday();
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
