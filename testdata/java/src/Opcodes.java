import java.util.ArrayList;
import java.util.List;
import java.util.function.IntBinaryOperator;

// The fixture of the class file reader and the bytecode decoder: one class that makes a compiler
// write most of the machine's instructions. `testdata/java/build.ps1` compiles it for every
// release the installed JDK targets and keeps `javap`'s listing beside each class.
public class Opcodes {
    static int counter;
    long total;
    final double[] samples = new double[4];

    Opcodes(long start) {
        total = start;
    }

    static int arithmetic(int a, int b) {
        int r = a + b * 3 - (a / (b | 1)) % 7;
        r ^= a << 3;
        r |= b >> 2;
        r &= ~(a >>> 1);
        r += 1000;
        return -r;
    }

    static long wide(long a, int b) {
        long r = a * 0x123456789L + b;
        r = (r << 5) ^ (r >>> 3) ^ (r >> b);
        return r % 1000003L - (r & 0xffL) | 1L;
    }

    static double real(float f, double d) {
        double r = f * 1.5f + d / 3.0 - (d % 2.0);
        float g = (float) r - f;
        return g > 0 ? -r : r + (long) g + (int) d;
    }

    static int convert(long l, double d, int i) {
        byte b = (byte) i;
        char c = (char) i;
        short s = (short) i;
        float f = l;
        return b + c + s + (int) f + (int) l + (int) (float) d + (l > d ? 1 : 0) + (f < d ? 2 : 0);
    }

    static int dense(int k) {
        switch (k) {
            case 0: return 10;
            case 1: return 11;
            case 2: return 12;
            case 3: return 13;
            default: return -1;
        }
    }

    static int sparse(int k) {
        switch (k) {
            case -100: return 1;
            case 7: return 2;
            case 9000: return 3;
            default: return 0;
        }
    }

    static int words(String s) {
        switch (s) {
            case "one": return 1;
            case "two": return 2;
            default: return s.length();
        }
    }

    static int[][] arrays(int n) {
        int[][] grid = new int[n][n + 1];
        byte[] bytes = new byte[n];
        char[] chars = new char[n];
        short[] shorts = new short[n];
        long[] longs = new long[n];
        float[] floats = new float[n];
        boolean[] flags = new boolean[n];
        String[] names = new String[n];
        for (int i = 0; i < n; i++) {
            bytes[i] = (byte) i;
            chars[i] = (char) (bytes[i] + 'a');
            shorts[i] = (short) chars[i];
            longs[i] = shorts[i];
            floats[i] = longs[i];
            flags[i] = floats[i] > 1;
            names[i] = flags[i] ? "y" : null;
            grid[i][i] = names.length + grid[i].length;
        }
        return grid;
    }

    synchronized long guarded(Object o) {
        synchronized (o) {
            total++;
        }
        try {
            if (o instanceof String) {
                return ((String) o).length();
            }
            throw new IllegalStateException("no");
        } catch (IllegalStateException e) {
            return -1;
        } finally {
            counter++;
        }
    }

    static int calls(List<String> list, CharSequence text) {
        IntBinaryOperator add = (x, y) -> x + y;
        Runnable r = () -> counter++;
        r.run();
        list.add("n" + text.length() + counter);
        return add.applyAsInt(list.size(), text == null ? 0 : 1);
    }

    double fields() {
        samples[0] = total;
        samples[1] += 2.5;
        counter = (int) samples[0];
        return samples[1] * counter;
    }

    public static void main(String[] args) {
        Opcodes o = new Opcodes(40);
        List<String> l = new ArrayList<>();
        System.out.println(arithmetic(3, 4) + wide(5, 6) + real(1f, 2.0) + convert(7, 8.0, 9) + dense(2) + sparse(7) + words("two")
            + arrays(3).length + o.guarded("s") + calls(l, "abc") + o.fields());
    }
}
