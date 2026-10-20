/* Unmodified consumer: links libgsl.so (our stub) and calls gsl_poly_solve_quadratic /
 * gsl_poly_solve_cubic with no idea the math runs inside the lind sandbox. */
#include <stdio.h>

int gsl_poly_solve_quadratic(double a, double b, double c, double *x0, double *x1);
int gsl_poly_solve_cubic(double a, double b, double c, double *x0, double *x1, double *x2);

int main(void) {
    double x0, x1, x2;

    /* x^2 - 5x + 6 = 0  ->  roots 2, 3 */
    int n = gsl_poly_solve_quadratic(1.0, -5.0, 6.0, &x0, &x1);
    printf("quadratic x^2-5x+6: %d real roots: %g, %g   (expect 2: 2, 3)\n", n, x0, x1);

    /* x^3 - 6x^2 + 11x - 6 = 0  ->  roots 1, 2, 3 */
    n = gsl_poly_solve_cubic(-6.0, 11.0, -6.0, &x0, &x1, &x2);
    printf("cubic x^3-6x^2+11x-6: %d real roots: %g, %g, %g   (expect 3: 1, 2, 3)\n", n, x0, x1, x2);
    return 0;
}
