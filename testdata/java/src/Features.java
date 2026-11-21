import java.util.ArrayList;
import java.util.List;

// The fixture of what Java 5 brought, as every later compiler writes it: enums and a switch over
// one, the for-each loop over an array and over a list, boxes made and opened by the compiler,
// variable arguments, generic methods, and a class initialized when it is first used and not
// before. `testdata/java/build.ps1` compiles it for Java 8 and for the newest release into
// `features/<release>/`.
public class Features {
    enum Planet {
        MERCURY(4), VENUS(9), EARTH(10);

        final int gravity;

        Planet(int gravity) {
            this.gravity = gravity;
        }

        int weigh(int mass) {
            return mass * gravity / 10;
        }
    }

    static class Late {
        static int started = announce("late starts");

        static int announce(String what) {
            System.out.println(what);
            return 7;
        }
    }

    static int sum(int... values) {
        int total = 0;
        for (int value : values) {
            total += value;
        }
        return total;
    }

    static <T> T last(List<T> items) {
        return items.get(items.size() - 1);
    }

    static String describe(Planet planet) {
        switch (planet) {
            case MERCURY: return "hot";
            case EARTH: return "home";
            default: return "far";
        }
    }

    public static void main(String[] args) {
        System.out.println("main starts");
        for (Planet planet : Planet.values()) {
            System.out.println(planet + " " + planet.ordinal() + " " + planet.name() + " " + planet.weigh(70) + " " + describe(planet));
        }
        System.out.println(Planet.valueOf("VENUS").gravity + " " + (Planet.EARTH == Planet.valueOf("EARTH")) + " " + Planet.EARTH.compareTo(Planet.MERCURY));
        List<Integer> numbers = new ArrayList<>();
        for (int i = 1; i <= 5; i++) {
            numbers.add(i * i);
        }
        int total = 0;
        for (Integer number : numbers) {
            total += number;
        }
        Integer boxed = 127;
        Long wide = 1L << 40;
        Double real = 2.5;
        Character letter = 'x';
        Boolean flag = total > 50;
        System.out.println(total + " " + last(numbers) + " " + sum() + " " + sum(1, 2, 3) + " " + numbers);
        System.out.println(boxed + 1 + " " + (wide + 1) + " " + real * 2 + " " + letter + " " + flag + " " + boxed.equals(127) + " " + numbers.contains(16));
        System.out.println("before late");
        System.out.println(Late.started + Late.announce("again"));
    }
}
