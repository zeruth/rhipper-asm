// The fixture of Java 21: a switch over the type of its operand, which is a call site of
// invokedynamic that says which case an object is of, with guards, record patterns that take
// their operand apart, a sealed interface whose switch needs no default, and a null case.
// `testdata/java/build.ps1` compiles it for Java 21 and for the newest release into
// `patterns/<release>/`.
public class Patterns {
    sealed interface Shape permits Circle, Rect, Group {
    }

    record Circle(double radius) implements Shape {
    }

    record Rect(double width, double height) implements Shape {
    }

    record Group(Shape first, Shape second) implements Shape {
    }

    static double area(Shape shape) {
        return switch (shape) {
            case Circle c -> 3.0 * c.radius() * c.radius();
            case Rect(double width, double height) -> width * height;
            case Group(Shape first, Shape second) -> area(first) + area(second);
        };
    }

    static String tell(Object o) {
        return switch (o) {
            case null -> "nothing";
            case Integer i when i > 100 -> "a big number";
            case Integer i -> "the number " + i;
            case String s when s.isEmpty() -> "no text";
            case String s -> "text of " + s.length();
            case Group(Circle c, Rect r) -> "a circle of " + c.radius() + " and a rectangle";
            case Shape s -> "a shape of " + area(s);
            default -> "something else";
        };
    }

    public static void main(String[] args) {
        Shape both = new Group(new Circle(1.0), new Rect(2.0, 3.5));
        System.out.println(area(both) + " " + area(new Group(both, both)));
        Object[] things = { null, 7, 700, "", "seven", both, new Group(new Rect(1, 1), new Circle(2)), new Circle(0.5), 2.5 };
        for (Object thing : things) {
            System.out.println(tell(thing));
        }
    }
}
