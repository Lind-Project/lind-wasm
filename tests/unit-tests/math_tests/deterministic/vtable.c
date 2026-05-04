#include <math.h>
#include <stdlib.h>
#include <stdio.h>

#define EPS 1e-6

typedef struct ShapeVTable {
    double (*area)(void *self);
} ShapeVTable;

typedef struct Shape {
    const ShapeVTable *vtable;
} Shape;

typedef struct {
    Shape base;
    double width;
    double height;
} Rectangle;

typedef struct {
    Shape base;
    double radius;
} Circle;

double rectangle_area(void *self) {
    Rectangle *r = (Rectangle *)self;
    return r->width * r->height;
}

double circle_area(void *self) {
    Circle *c = (Circle *)self;
    return M_PI * c->radius * c->radius;
}

ShapeVTable rectangle_vtable = { rectangle_area };
ShapeVTable circle_vtable = { circle_area };

Rectangle *create_rectangle(double width, double height) {
    Rectangle *r = malloc(sizeof(Rectangle));
    r->base.vtable = &rectangle_vtable;
    r->width = width;
    r->height = height;
    return r;
}

Circle *create_circle(double radius) {
    Circle *c = malloc(sizeof(Circle));
    c->base.vtable = &circle_vtable;
    c->radius = radius;
    return c;
}

double shape_area(Shape *shape) {
    return shape->vtable->area(shape);
}

int main(void) {
    Rectangle *rect = create_rectangle(3.0, 4.0);
    Circle *circ = create_circle(2.5);

    if (rect->base.vtable->area != rectangle_area) {
        fprintf(stderr, "vtable test failed: rectangle vtable does not dispatch to rectangle_area\n");
        free(rect);
        free(circ);
        return 1;
    }
    if (circ->base.vtable->area != circle_area) {
        fprintf(stderr, "vtable test failed: circle vtable does not dispatch to circle_area\n");
        free(rect);
        free(circ);
        return 1;
    }

    double rect_area = shape_area((Shape *)rect);
    double circ_area = shape_area((Shape *)circ);

    if (fabs(rect_area - 12.0) > EPS) {
        fprintf(stderr, "vtable test failed: rectangle area mismatch\n");
        free(rect);
        free(circ);
        return 1;
    }
    double expected_circ = M_PI * 2.5 * 2.5;
    if (fabs(circ_area - expected_circ) > EPS) {
        fprintf(stderr, "vtable test failed: circle area mismatch\n");
        free(rect);
        free(circ);
        return 1;
    }

    free(rect);
    free(circ);
    return 0;
}
