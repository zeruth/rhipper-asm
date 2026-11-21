// The fixture of the machine's exceptions: what the machine throws itself (a division by zero, an
// index outside an array, a null used, a cast that does not hold), each caught by its class, a
// `finally` on both ways out, an exception of the program's own class, and one nothing catches.
// `testdata/java/build.ps1` compiles it once, for Java 8, into `thrown/`.
public class Thrown {
    static class Mine extends RuntimeException {
        final int code;

        Mine(String message, int code) {
            super(message);
            this.code = code;
        }
    }

    interface Shape {
        double area();

        default String name() {
            return "shape " + area();
        }
    }

    static int divide(int a, int b) {
        try {
            return a / b;
        } catch (ArithmeticException e) {
            System.out.println("caught " + e.getMessage());
            return -1;
        } finally {
            System.out.println("divided " + a);
        }
    }

    static int index(int[] values, int at) {
        try {
            return values[at];
        } catch (ArrayIndexOutOfBoundsException e) {
            System.out.println(e.getMessage());
            return -2;
        }
    }

    static int length(String s) {
        try {
            return s.length();
        } catch (NullPointerException e) {
            return -3;
        }
    }

    static String cast(Object o) {
        try {
            return (String) o;
        } catch (ClassCastException e) {
            return "not a string";
        }
    }

    static int deep(int n) {
        if (n == 0) {
            throw new Mine("deep", 7);
        }
        return deep(n - 1) + 1;
    }

    public static void main(String[] args) {
        System.out.println(divide(7, 2) + divide(7, 0));
        System.out.println(index(new int[3], 1) + index(new int[3], 3));
        System.out.println(length("four") + length(null));
        System.out.println(cast("s") + cast(Integer.valueOf(1)));
        try {
            deep(5);
        } catch (Mine e) {
            System.out.println(e.getMessage() + e.code);
        }
        Shape square = () -> 4.0;
        System.out.println(square.name());
        long big = Long.MIN_VALUE;
        System.out.println(big / -1 + " " + Integer.MIN_VALUE % -1 + " " + (int) 1e20 + " " + (long) Double.NaN + " " + (1 << 33) + " " + 1e-5 + " " + 1.0e7f + " " + 100.0);
        throw new IllegalStateException(String.valueOf(args.length + 3));
    }
}
