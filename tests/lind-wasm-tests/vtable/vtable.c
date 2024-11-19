#include <stdio.h>
#include <stdlib.h>
#include <math.h>

// Base vtable definition
typedef struct ShapeVTable {
    double (*area)(void *self);
} ShapeVTable;

// Base structure definition
typedef struct Shape {
    const ShapeVTable *vtable;
} Shape;

// Rectangle structure definition
typedef struct {
    Shape base;
    double width;
    double height;
} Rectangle;

// Circle structure definition
typedef struct {
    Shape base;
    double radius;
} Circle;

// Function implementations for Rectangle
double rectangle_area(void *self) {
    Rectangle *rectangle = (Rectangle *)self;
    return rectangle->width * rectangle->height;
}

// Function implementations for Circle
double circle_area(void *self) {
    Circle *circle = (Circle *)self;
    return M_PI * circle->radius * circle->radius;
}

// Vtable instances
ShapeVTable rectangle_vtable = { rectangle_area };
ShapeVTable circle_vtable = { circle_area };

// Helper functions to create shapes
Rectangle *create_rectangle(double width, double height) {
    Rectangle *rectangle = malloc(sizeof(Rectangle));
    rectangle->base.vtable = &rectangle_vtable;
    rectangle->width = width;
    rectangle->height = height;
    return rectangle;
}

Circle *create_circle(double radius) {
    Circle *circle = malloc(sizeof(Circle));
    circle->base.vtable = &circle_vtable;
    circle->radius = radius;
    return circle;
}

// Function to calculate area using dynamic dispatch
double shape_area(Shape *shape) {
    return shape->vtable->area(shape);
}

// Main function to demonstrate the usage
int main() {
    Shape *shapes[2];
    shapes[0] = (Shape *)create_rectangle(3.0, 4.0);
    shapes[1] = (Shape *)create_circle(2.5);

    for (int i = 0; i < 2; i++) {
        printf("Shape %d area: %f\n", i, shape_area(shapes[i]));
    }

    // Clean up
    for (int i = 0; i < 2; i++) {
        free(shapes[i]);
    }

    return 0;
}

