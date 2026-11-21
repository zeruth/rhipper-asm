// The fixture of what the releases from Java 10 to Java 17 brought to a class file: `var`, a
// switch that is an expression, text blocks, a type test that binds, and records, whose
// `toString`, `equals` and `hashCode` are call sites of invokedynamic the library's bootstrap
// method answers. `testdata/java/build.ps1` compiles it for Java 17 and for the newest release
// into `records/<release>/`.
public class Records {
    record Point(int x, int y) {
        int manhattan() {
            return Math.abs(x) + Math.abs(y);
        }
    }

    record Line(Point from, Point to, String name, double width, long id, boolean dashed) {
    }

    static int days(String month) {
        return switch (month) {
            case "february" -> 28;
            case "april", "june", "september", "november" -> 30;
            default -> {
                int length = month.length();
                yield length > 20 ? -1 : 31;
            }
        };
    }

    static String tell(Object o) {
        if (o instanceof Point p && p.x() > 0) {
            return "a point right of the origin by " + p.x();
        }
        if (o instanceof String s) {
            return "text of " + s.length();
        }
        return "something else";
    }

    public static void main(String[] args) {
        var a = new Point(3, -4);
        var b = new Point(3, -4);
        var line = new Line(a, new Point(0, 0), "edge", 1.5, 1L << 33, true);
        System.out.println(a + " " + a.manhattan() + " " + a.equals(b) + " " + a.equals(line.to()) + " " + a.equals("no") + " " + (a.hashCode() == b.hashCode()));
        System.out.println(line);
        System.out.println(line.equals(new Line(b, new Point(0, 0), "edge", 1.5, 1L << 33, true)) + " " + line.equals(new Line(b, new Point(0, 0), "rim", 1.5, 1L << 33, true)));
        System.out.println(days("february") + days("june") + days("july"));
        System.out.println(tell(a) + "; " + tell(new Point(-1, 0)) + "; " + tell("four") + "; " + tell(null));
        String block = """
            two lines
              of text""";
        System.out.println(block.length() + " " + block);
    }
}
