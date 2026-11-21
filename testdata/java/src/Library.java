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

    public static void main(String[] args) {
        System.out.println(strings());
        System.out.println(builder());
        System.out.println(numbers());
        System.out.println(characters());
        System.out.println(lists());
        System.out.println(maps());
        System.out.println(arrays());
    }
}
