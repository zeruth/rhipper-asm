// The fixture of the releases no JDK compiles for any more: a program that uses nothing the first
// class files did not have (no lambda, no call site of invokedynamic, no constant of a later
// kind), so its classes are classes of every release once their version says so.
// `testdata/java/build.ps1` compiles it once, for Java 8, into `plain/`; the tests set the
// version of its class files back, release by release, and hold each to what `java` printed.
public class Plain {
    static final int[] PRIMES = new int[8];
    static int made;

    static {
        int found = 0;
        for (int n = 2; found < PRIMES.length; n++) {
            boolean prime = true;
            for (int d = 2; d * d <= n; d++) {
                if (n % d == 0) {
                    prime = false;
                    break;
                }
            }
            if (prime) {
                PRIMES[found++] = n;
            }
        }
    }

    interface Shape {
        double area();
    }

    static abstract class Named implements Shape {
        final String name;

        Named(String name) {
            this.name = name;
            made++;
        }

        public String toString() {
            return name + " of " + area();
        }
    }

    static class Square extends Named {
        final double side;

        Square(double side) {
            super("square");
            this.side = side;
        }

        public double area() {
            return side * side;
        }
    }

    static class Circle extends Named {
        final double radius;

        Circle(double radius) {
            super("circle");
            this.radius = radius;
        }

        public double area() {
            return 3.0 * radius * radius;
        }

        public String toString() {
            return "round " + super.toString();
        }
    }

    static long factorial(int n) {
        return n <= 1 ? 1 : n * factorial(n - 1);
    }

    static int weigh(String word) {
        switch (word) {
            case "light": return 1;
            case "heavy": return 100;
            default: return word.length();
        }
    }

    static String reverse(String text) {
        StringBuilder out = new StringBuilder();
        for (int i = text.length() - 1; i >= 0; i--) {
            out.append(text.charAt(i));
        }
        return out.toString();
    }

    static int risky(int[] values, int at, int by) {
        int result = 0;
        try {
            result = values[at] / by;
        } catch (ArithmeticException e) {
            result = -1;
        } catch (ArrayIndexOutOfBoundsException e) {
            result = -2;
        } finally {
            made += 10;
        }
        return result;
    }

    public static void main(String[] args) {
        Shape[] shapes = new Shape[] { new Square(1.5), new Circle(2.0), new Square(3.0) };
        double total = 0;
        for (int i = 0; i < shapes.length; i++) {
            System.out.println(shapes[i]);
            total += shapes[i].area();
        }
        System.out.println(total);
        System.out.println(PRIMES[7] + " " + factorial(15) + " " + made);
        System.out.println(weigh("light") + weigh("heavy") + weigh("neither"));
        System.out.println(reverse("stressed"));
        System.out.println(risky(PRIMES, 3, 2) + " " + risky(PRIMES, 3, 0) + " " + risky(PRIMES, 9, 1) + " " + made);
        long bits = 0x0123456789abcdefL;
        System.out.println((bits >>> 13) ^ (bits << 7) ^ (int) (bits >> 40));
        System.out.println((float) total / 7 + " " + (char) ('a' + PRIMES[2]) + " " + (shapes[1] instanceof Named) + " " + (shapes[0] instanceof Circle));
    }
}
