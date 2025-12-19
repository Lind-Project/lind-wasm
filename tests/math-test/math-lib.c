// #include <math.h>
// #include <stdio.h>

// int main(void) {
//     double x = 5.3;
//     double y = 2.0;

//     double r = fmod(x, y);

//     printf("fmod(%f, %f) = %.17g\n", x, y, r);

//     return 0;
// }

#include <math.h>
#include <stdio.h>
#include <float.h>
#include <stdbool.h>

/* ---------- basic test harness ---------- */

static int tests_run = 0;
static int tests_failed = 0;

#define TOL 1e-12

static bool nearly_equal(double a, double b, double tol) {
    if (isnan(a) && isnan(b)) return true;
    if (isinf(a) || isinf(b)) return a == b;

    double diff = fabs(a - b);
    if (diff <= tol) return true;

    double largest = fmax(fabs(a), fabs(b));
    return diff <= largest * tol;
}

#define CHECK_NEAR(desc, val, expect, tol) do {                   \
    tests_run++;                                                 \
    double _v = (val);                                           \
    double _e = (expect);                                        \
    bool ok = nearly_equal(_v, _e, (tol));                       \
                                                                 \
    if (!ok) {                                                   \
        tests_failed++;                                          \
        printf("FAIL: %s\n", (desc));                            \
    } else {                                                     \
        printf("PASS: %s\n", (desc));                            \
    }                                                            \
                                                                 \
    printf("  got     = %.17g\n", _v);                           \
    printf("  expected= %.17g\n", _e);                           \
} while (0)


#define CHECK_TRUE(desc, cond) do {                              \
    tests_run++;                                                 \
    bool ok = (cond);                                            \
                                                                 \
    if (!ok) {                                                   \
        tests_failed++;                                          \
        printf("FAIL: %s\n", (desc));                            \
    } else {                                                     \
        printf("PASS: %s\n", (desc));                            \
    }                                                            \
} while (0)


#ifndef M_PI
#define M_PI 3.14159265358979323846
#endif

#ifndef M_E
#define M_E 2.71828182845904523536
#endif

/* ---------- tests ---------- */

static void test_basic_unary(void) {
    printf("== test_basic_unary ==\n");

    CHECK_NEAR("fabs(-3.5) == 3.5", fabs(-3.5), 3.5, TOL);

    CHECK_NEAR("floor(2.9) == 2.0", floor(2.9), 2.0, TOL);
    CHECK_NEAR("ceil(2.1) == 3.0", ceil(2.1), 3.0, TOL);

    CHECK_NEAR("trunc(2.9) == 2.0", trunc(2.9), 2.0, TOL);
    CHECK_NEAR("trunc(-2.9) == -2.0", trunc(-2.9), -2.0, TOL);

    CHECK_NEAR("round(2.4) == 2.0", round(2.4), 2.0, TOL);
    CHECK_NEAR("round(2.5) == 3.0", round(2.5), 3.0, TOL);
    CHECK_NEAR("round(-2.5) == -3.0", round(-2.5), -3.0, TOL);
}

static void test_sqrt_cbrt_hypot(void) {
    printf("== test_sqrt_cbrt_hypot ==\n");

    CHECK_NEAR("sqrt(4.0) == 2.0", sqrt(4.0), 2.0, TOL);
    CHECK_NEAR("sqrt(2)^2 ~= 2", pow(sqrt(2.0), 2.0), 2.0, 1e-12);

    CHECK_TRUE("sqrt(-1) is NaN", isnan(sqrt(-1.0)));

    CHECK_NEAR("cbrt(8.0) == 2.0", cbrt(8.0), 2.0, TOL);
    CHECK_NEAR("cbrt(-8.0) == -2.0", cbrt(-8.0), -2.0, TOL);

    CHECK_NEAR("hypot(3,4) == 5", hypot(3.0, 4.0), 5.0, 1e-15);
}

static void test_exp_log(void) {
    printf("== test_exp_log ==\n");

    CHECK_NEAR("exp(0) == 1", exp(0.0), 1.0, TOL);
    CHECK_NEAR("exp(1) ~= e", exp(1.0), M_E, 1e-15);

    double v = 3.141592653589793;
    CHECK_NEAR("log(exp(v)) ~= v", log(exp(v)), v, 1e-12);

    CHECK_NEAR("log(1) == 0", log(1.0), 0.0, TOL);
    CHECK_NEAR("log10(1) == 0", log10(1.0), 0.0, TOL);
    CHECK_NEAR("pow(10, 2) == 100", pow(10.0, 2.0), 100.0, 1e-12);

    double lz = log(0.0);
    CHECK_TRUE("log(0) == -INF", isinf(lz) && lz < 0);

    double lnneg = log(-1.0);
    CHECK_TRUE("log(-1) is NaN", isnan(lnneg));

    CHECK_NEAR("exp2(10) == 1024", exp2(10.0), 1024.0, 1e-10);
}

static void test_trig(void) {
    printf("== test_trig ==\n");

    double tol = 1e-12;

    CHECK_NEAR("sin(0) == 0", sin(0.0), 0.0, tol);
    CHECK_NEAR("cos(0) == 1", cos(0.0), 1.0, tol);

    CHECK_NEAR("sin(pi/2) ~= 1", sin(M_PI/2), 1.0, 1e-12);
    CHECK_NEAR("cos(pi/2) ~= 0", cos(M_PI/2), 0.0, 1e-12);

    CHECK_NEAR("sin(pi) ~= 0", sin(M_PI), 0.0, 1e-12);
    CHECK_NEAR("cos(pi) ~= -1", cos(M_PI), -1.0, 1e-12);

    double x = 0.3;
    double s = sin(x);
    double c = cos(x);
    CHECK_NEAR("sin^2(x)+cos^2(x) ~= 1", s*s + c*c, 1.0, 1e-12);

    CHECK_NEAR("tan(0.3) ~= sin/cos", tan(x), s/c, 1e-12);

    CHECK_NEAR("atan(1) ~= pi/4", atan(1.0), M_PI/4, 1e-12);
    CHECK_NEAR("atan2(1,1) ~= pi/4", atan2(1.0, 1.0), M_PI/4, 1e-12);
    CHECK_NEAR("atan2(1,-1) ~= 3pi/4", atan2(1.0, -1.0), 3*M_PI/4, 1e-12);
}

static void test_hyperbolic(void) {
    printf("== test_hyperbolic ==\n");

    CHECK_NEAR("sinh(0) == 0", sinh(0.0), 0.0, TOL);
    CHECK_NEAR("cosh(0) == 1", cosh(0.0), 1.0, TOL);
    CHECK_NEAR("tanh(0) == 0", tanh(0.0), 0.0, TOL);

    double x = 1.0;
    double sh = sinh(x);
    double ch = cosh(x);

    CHECK_NEAR("cosh^2(x)-sinh^2(x) ~= 1",
               ch*ch - sh*sh, 1.0, 1e-12);
}

static void test_mod_remainder(void) {
    printf("== test_mod_remainder ==\n");

    /* fmod */
    CHECK_NEAR("fmod(5.3,2.0) ~= 1.3", fmod(5.3, 2.0), 1.3, 1e-12);

    double r1 = fmod(-5.3, 2.0);
    CHECK_TRUE("fmod(-5.3,2.0) < 0", r1 < 0);
    CHECK_NEAR("|fmod(-5.3,2.0)| ~= 1.3", fabs(r1), 1.3, 1e-12);

    /* remainder: ties-to-even remainder */
    double q = remainder(5.0, 2.0);  /* 5 = 2*2 + 1 */
    CHECK_NEAR("remainder(5,2) == 1", q, 1.0, 1e-12);

    q = remainder(5.0, -2.0);
    CHECK_NEAR("remainder(5,-2) == 1", q, 1.0, 1e-12);

    q = remainder(4.5, 2.0); /* 4.5 / 2 = 2.25 => nearest integer 2 => remainder 0.5 */
    CHECK_NEAR("remainder(4.5,2) ~= 0.5", q, 0.5, 1e-12);
}

static void test_special_values(void) {
    printf("== test_special_values ==\n");

    double nanv  = NAN;
    double inf   = INFINITY;
    double ninf  = -INFINITY;
    double z     = 0.0;
    double mz    = -0.0;
    double norm  = 1.0;
    double sub   = DBL_MIN / 2.0;

    CHECK_TRUE("isnan(NAN)", isnan(nanv));
    CHECK_TRUE("isinf(INFINITY)", isinf(inf) && inf > 0);
    CHECK_TRUE("isinf(-INFINITY)", isinf(ninf) && ninf < 0);

    CHECK_TRUE("fpclassify(0) == FP_ZERO", fpclassify(z) == FP_ZERO);
    CHECK_TRUE("fpclassify(-0) == FP_ZERO", fpclassify(mz) == FP_ZERO);
    CHECK_TRUE("signbit(-0) set", signbit(mz));
    CHECK_TRUE("signbit(+0) not set", !signbit(z));

    CHECK_TRUE("fpclassify(1.0) == FP_NORMAL", fpclassify(norm) == FP_NORMAL);

    CHECK_TRUE("subnormal classified as FP_SUBNORMAL",
               fpclassify(sub) == FP_SUBNORMAL);
}

/* ---------- main ---------- */

int main(void) {
    test_basic_unary();
    test_trig();
    test_hyperbolic();
    test_mod_remainder();
    test_special_values();
    test_sqrt_cbrt_hypot();
    test_exp_log();

    printf("\nTests run: %d\n", tests_run);
    if (tests_failed == 0) {
        printf("ALL TESTS PASSED \n");
        return 0;
    } else {
        printf("Tests failed: %d \n", tests_failed);
        return 1;
    }
}
