import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

// The fixture of the library a program leans on every day: what strings and their builder do,
// the arithmetic of `Math`, numbers read from text and written to it, characters told apart, a
// list changed in place, a map from keys to values, and arrays copied. Both runtimes answer for
// each call here: the interpreter's and the port's. `testdata/java/build.ps1` compiles it for
// Java 8 into `library/`.
public class Library {
    static String strings() {
        String s = "  The Quick Brown Fox  ";
        String t = s.trim();
        StringBuilder out = new StringBuilder();
        out.append(t.length()).append(' ').append(t.substring(4, 9)).append(' ').append(t.substring(16));
        out.append(' ').append(t.indexOf('Q')).append(' ').append(t.indexOf("Brown")).append(' ').append(t.indexOf("cat"));
        out.append(' ').append(t.contains("ick")).append(t.startsWith("The")).append(t.endsWith("fox"));
        out.append(' ').append(t.toUpperCase()).append(' ').append(t.toLowerCase()).append(' ').append(t.replace('o', '0'));
        out.append(' ').append("apple".compareTo("apricot")).append(' ').append("b".compareTo("a")).append(' ').append("same".equals("sa" + "me"));
        out.append(' ').append(t.charAt(4)).append(t.isEmpty()).append("".isEmpty()).append(' ').append(t.lastIndexOf('o'));
        return out.toString();
    }

    static String builder() {
        StringBuilder b = new StringBuilder("hello");
        b.append(", ").append("world").append('!').append(42).append(1.5).append(true).append(7L);
        String forward = b.toString();
        b.reverse();
        b.setLength(5);
        b.insert(0, "<").append('>');
        return forward + " " + b + " " + b.length() + " " + b.charAt(1);
    }

    static String numbers() {
        int parsed = Integer.parseInt("-1234") + Integer.parseInt("56");
        long wide = Long.parseLong("9000000000") / 3;
        double root = Math.sqrt(2.0) * Math.sqrt(2.0);
        return parsed + " " + wide + " " + Math.abs(-7) + " " + Math.max(3, 9) + " " + Math.min(-2L, 5L) + " " + Math.pow(2.0, 10.0) + " "
            + Math.floor(-1.5) + " " + Math.ceil(1.2) + " " + Math.round(2.5) + " " + (root > 1.99 && root < 2.01) + " " + Integer.toString(255) + " "
            + Integer.toHexString(255) + " " + Integer.MAX_VALUE + " " + Integer.valueOf(12).hashCode() + " " + Double.parseDouble("2.5") * 2 + " "
            + String.valueOf(3.0f) + " " + Long.toString(-5L) + " " + Math.abs(-2.5) + " " + Integer.compare(3, 7) + " " + Boolean.parseBoolean("true");
    }

    static String characters() {
        StringBuilder out = new StringBuilder();
        for (char c : "a1 Z_".toCharArray()) {
            out.append(Character.isDigit(c) ? 'd' : Character.isLetter(c) ? 'l' : Character.isWhitespace(c) ? 'w' : 'o');
            out.append(Character.toUpperCase(c)).append(Character.toLowerCase(c));
        }
        return out.append(' ').append(Character.isUpperCase('Q')).append(Character.isLowerCase('Q')).append((int) Character.forDigit(11, 16)).toString();
    }

    static String lists() {
        List<String> names = new ArrayList<>();
        names.add("delta");
        names.add("alpha");
        names.add("charlie");
        names.add(1, "bravo");
        names.set(0, "echo");
        String removed = names.remove(2);
        boolean gone = names.remove("nobody");
        StringBuilder out = new StringBuilder();
        for (String name : names) {
            out.append(name.charAt(0));
        }
        out.append(' ').append(removed).append(gone).append(names.indexOf("charlie")).append(names.contains("echo")).append(names.size());
        List<Integer> squares = new ArrayList<>();
        for (int i = 0; i < 6; i++) {
            squares.add(i * i);
        }
        squares.remove(Integer.valueOf(9));
        out.append(' ').append(squares).append(squares.get(3)).append(squares.isEmpty());
        squares.clear();
        return out.append(squares.isEmpty()).append(squares.size()).toString();
    }

    static String maps() {
        Map<String, Integer> counts = new HashMap<>();
        String[] words = { "to", "be", "or", "not", "to", "be" };
        for (String word : words) {
            Integer seen = counts.get(word);
            counts.put(word, seen == null ? 1 : seen + 1);
        }
        Map<Integer, String> names = new HashMap<>();
        names.put(1, "one");
        names.put(2, "two");
        String old = names.put(1, "uno");
        return counts.get("to") + " " + counts.get("not") + " " + counts.get("nope") + " " + counts.size() + " " + counts.containsKey("or") + " "
            + counts.containsKey("and") + " " + counts.getOrDefault("and", -1) + " " + names.get(1) + " " + old + " " + names.remove(2) + " "
            + names.size() + " " + names.isEmpty();
    }

    static String arrays() {
        int[] from = { 1, 2, 3, 4, 5 };
        int[] to = new int[7];
        System.arraycopy(from, 1, to, 2, 3);
        int[] copy = from.clone();
        copy[0] = 9;
        StringBuilder out = new StringBuilder();
        for (int v : to) {
            out.append(v);
        }
        char[] letters = "hello".toCharArray();
        letters[0] = 'j';
        return out.append(' ').append(from[0]).append(copy[0]).append(' ').append(new String(letters)).append(String.valueOf(letters, 1, 3)).toString();
    }

    // Text made by a format, split by a separator, and written in another base; arrays sorted;
    // and an object asked its class.
    static String formats() {
        StringBuilder out = new StringBuilder();
        out.append(String.format("%d|%5d|%-5d|%05d|%+d|%x|%08x", 42, -7, 7, -42, 9, 255, 0xbeef));
        out.append(' ').append(String.format("%s|%6s|%-6s|%.2s|%c|%b|%b%%", "text", "ab", "cd", "long", 'z', true, null));
        out.append(' ').append(String.format("%.2f|%8.3f|%.0f|%f|%.2f|%.1f|%d", 2.5, -3.14159, 2.5, 1.5, 1.005, 0.05, 1L << 40));
        String[] parts = "a,b,,c,,".split(",");
        out.append(' ').append(parts.length);
        for (String part : parts) {
            out.append('[').append(part).append(']');
        }
        out.append(' ').append("one--two--three".split("--")[2]).append("none".split(";").length).append(",".split(",").length);
        out.append(' ').append(Long.toHexString(1L << 40)).append(' ').append(Integer.toBinaryString(10)).append(' ');
        out.append(Integer.toBinaryString(-1).length()).append(' ').append(Integer.toOctalString(64)).append(' ').append(Long.toBinaryString(5L));
        int[] numbers = {3, -1, 4, 1, -5, 9, 2, 6};
        java.util.Arrays.sort(numbers);
        char[] letters = "hello".toCharArray();
        java.util.Arrays.sort(letters);
        long[] wide = {1L << 40, -1L, 0L};
        java.util.Arrays.sort(wide);
        out.append(' ');
        for (int n : numbers) {
            out.append(n);
        }
        out.append(' ').append(new String(letters)).append(' ').append(wide[0]).append(wide[2]);
        Object text = "text";
        Object list = new ArrayList<String>();
        out.append(' ').append(text.getClass().getSimpleName()).append(' ').append(list.getClass().getName());
        return out.toString();
    }

    // Methods named as functions, whose numbers go in and out of their boxes on the way; and
    // more of what numbers, characters, strings and a builder do.
    static String references() {
        java.util.function.BiFunction<Integer, Integer, Integer> sum = Integer::sum;
        java.util.function.Function<String, Integer> length = String::length;
        java.util.function.Predicate<String> empty = String::isEmpty;
        java.util.function.ToIntFunction<Integer> twice = v -> v * 2;
        java.util.function.Supplier<Long> big = () -> 1L << 33;
        java.util.function.Function<Double, Boolean> positive = v -> v > 0;
        StringBuilder out = new StringBuilder();
        out.append(sum.apply(2, 3)).append(length.apply("four")).append(empty.test("")).append(twice.applyAsInt(21)).append(big.get());
        out.append(positive.apply(-1.5));
        out.append(' ').append(Integer.max(3, 9)).append(Integer.min(3, 9)).append(Integer.bitCount(255)).append(Integer.signum(-5));
        out.append(' ').append(Integer.parseInt("ff", 16)).append(Integer.parseInt("-101", 2)).append(Double.compare(1.0, 2.0));
        out.append(Double.compare(0.0, -0.0)).append(Double.compare(Double.NaN, 1.0));
        out.append(' ').append(Math.floorMod(-7, 3)).append(Math.floorDiv(-7, 3)).append(Math.floorMod(7, -3)).append(Math.floorDiv(7, 3));
        out.append(' ').append(Character.getNumericValue('7')).append(Character.getNumericValue('z')).append(Character.getNumericValue('-'));
        out.append(Character.isLetterOrDigit('x')).append(Character.isLetterOrDigit('_'));
        out.append(' ').append(java.util.Objects.hash(1, "a", null)).append(java.util.Objects.toString(null));
        out.append(' ').append("Hello".equalsIgnoreCase("hELLO")).append("a-b-c".replace("-", "+")).append("aaa".replace("aa", "b"));
        StringBuilder b = new StringBuilder("hello");
        b.deleteCharAt(0);
        b.setCharAt(0, 'E');
        b.delete(1, 2);
        b.delete(2, 99);
        try {
            Integer.parseInt("12x", 16);
        } catch (NumberFormatException e) {
            b.append(' ').append(e.getMessage());
        }
        return out.append(' ').append(b).toString();
    }

    // A map walked by its keys, its values and its entries, a set, and lists sorted: the order a
    // map and a set are walked in is the library's own, by the keys' hashes.
    static String collections() {
        Map<String, Integer> ages = new HashMap<>();
        String[] names = {"zoe", "adam", "mia", "bob", "yuri", "carl", "lena", "dave", "kim", "eve", "noor", "finn", "omar", "gus"};
        for (int n = 0; n < names.length; n++) {
            ages.put(names[n], 20 + n);
        }
        ages.remove("bob");
        StringBuilder out = new StringBuilder();
        int total = 0;
        for (String name : ages.keySet()) {
            out.append(name.charAt(0));
        }
        for (int age : ages.values()) {
            total = total * 3 + age;
        }
        out.append(' ').append(total).append(' ');
        for (Map.Entry<String, Integer> entry : ages.entrySet()) {
            out.append(entry.getKey().length() + entry.getValue() % 10);
        }
        java.util.Set<Integer> seen = new java.util.HashSet<>();
        int[] numbers = {40, 7, 23, 7, 100, 16, 40, 1, 33};
        int fresh = 0;
        for (int n : numbers) {
            if (seen.add(n)) {
                fresh++;
            }
        }
        seen.remove(23);
        out.append(' ').append(fresh).append(seen.size()).append(seen.contains(7)).append(seen.contains(23)).append(' ');
        for (int n : seen) {
            out.append(n).append(',');
        }
        List<String> sorted = new ArrayList<>(java.util.Arrays.asList("pear", "fig", "apple", "kiwi"));
        java.util.Collections.sort(sorted);
        out.append(' ').append(sorted);
        sorted.sort((a, b) -> a.length() != b.length() ? a.length() - b.length() : b.compareTo(a));
        out.append(sorted);
        List<Integer> values = new ArrayList<>(seen);
        java.util.Collections.sort(values);
        values.removeIf(v -> v % 2 == 0);
        values.addAll(java.util.Arrays.asList(5, 3));
        values.sort(null);
        out.append(' ').append(values).append(Integer.valueOf(3).compareTo(7)).append(Long.valueOf(9L).compareTo(2L));
        return out.toString();
    }

    public static void main(String[] args) {
        System.out.println(collections());
        System.out.println(references());
        System.out.println(formats());
        System.out.println(strings());
        System.out.println(builder());
        System.out.println(numbers());
        System.out.println(characters());
        System.out.println(lists());
        System.out.println(maps());
        System.out.println(arrays());
    }
}
