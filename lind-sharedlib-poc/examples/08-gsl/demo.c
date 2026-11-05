/* Unmodified consumer: links libgsl.so (our stub) and calls gsl_poly_solve_quadratic /
 * gsl_poly_solve_cubic with no idea the math runs inside the lind sandbox. */
#include <stdio.h>
#include <stddef.h>

int gsl_poly_solve_quadratic(double a, double b, double c, double *x0, double *x1);
int gsl_poly_solve_cubic(double a, double b, double c, double *x0, double *x1, double *x2);
int gsl_poly_eval_derivs(const double c[], unsigned long lenc, double x,
                         double res[], unsigned long lenres);
/* sort + statistics: demo links ONLY our stub .so (no native archive), so these MUST
 * resolve to the sandboxed wrappers — a clean check independent of GSL's own test coverage. */
void   gsl_sort(double *data, size_t stride, size_t n);
double gsl_stats_mean(const double data[], size_t stride, size_t n);

int main(void) {
    double x0, x1, x2;

    /* x^2 - 5x + 6 = 0  ->  roots 2, 3 */
    int n = gsl_poly_solve_quadratic(1.0, -5.0, 6.0, &x0, &x1);
    printf("quadratic x^2-5x+6: %d real roots: %g, %g   (expect 2: 2, 3)\n", n, x0, x1);

    /* x^3 - 6x^2 + 11x - 6 = 0  ->  roots 1, 2, 3 */
    n = gsl_poly_solve_cubic(-6.0, 11.0, -6.0, &x0, &x1, &x2);
    printf("cubic x^3-6x^2+11x-6: %d real roots: %g, %g, %g   (expect 3: 1, 2, 3)\n", n, x0, x1, x2);

    /* P(x) = 1 + 2x + 3x^2  at x = 2  ->  P=17, P'=2+6x=14, P''=6.  (input + output arrays) */
    double coeffs[3] = {1.0, 2.0, 3.0};
    double res[3];
    gsl_poly_eval_derivs(coeffs, 3, 2.0, res, 3);
    printf("eval_derivs 1+2x+3x^2 @2: P=%g, P'=%g, P''=%g   (expect 17, 14, 6)\n",
           res[0], res[1], res[2]);

    /* sort: sorts the array in place (InOut) */
    double d[5] = {3.1, 1.2, 4.8, 1.5, 2.6};
    gsl_sort(d, 1, 5);
    printf("gsl_sort -> %g %g %g %g %g   (expect 1.2 1.5 2.6 3.1 4.8)\n",
           d[0], d[1], d[2], d[3], d[4]);

    /* statistics: reduction array -> scalar */
    double s[5] = {2.0, 4.0, 4.0, 4.0, 6.0};
    printf("gsl_stats_mean -> %g   (expect 4)\n", gsl_stats_mean(s, 1, 5));
    return 0;
}

