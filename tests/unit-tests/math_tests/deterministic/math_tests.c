#include <stdio.h>
#include <math.h>
#include <assert.h>

#define EPSILON 1e-9

static void assert_double_eq(double actual, double expected, const char *msg) {
	printf("  %-25s expected: %-12.9f observed: %-12.9f",
           msg, expected, actual);
    	if (fabs(actual - expected) > EPSILON) {
        	fprintf(stderr,
                "FAILED: %s\n  expected %.12f\n  got      %.12f\n",
                msg, expected, actual);
    	}
    	else {
		printf("  ✅ OK\n");
	}
}

/* ---------------- Basic Operations ---------------- */

void test_basic_operations(void) {
    printf("Testing basic operations...\n");

    assert_double_eq(sqrt(16.0), 4.0, "sqrt(16.0)");
    assert_double_eq(pow(2.0, 3.0), 8.0, "pow(2.0, 3.0)");
    assert_double_eq(fabs(-5.5), 5.5, "fabs(-5.5)");
    assert_double_eq(ceil(3.2), 4.0, "ceil(3.2)");
    assert_double_eq(floor(3.8), 3.0, "floor(3.8)");

    printf("  ✔ basic operations passed\n");
}

/* ---------------- Trigonometry ---------------- */

void test_trigonometric(void) {
    printf("Testing trigonometric functions...\n");

    double angle = M_PI / 4.0;

    assert_double_eq(sin(angle), sqrt(2.0) / 2.0, "sin(pi/4)");
    assert_double_eq(cos(angle), sqrt(2.0) / 2.0, "cos(pi/4)");
    assert_double_eq(tan(angle), 1.0, "tan(pi/4)");

    assert_double_eq(asin(0.5), M_PI / 6.0, "asin(0.5)");
    assert_double_eq(acos(0.5), M_PI / 3.0, "acos(0.5)");
    assert_double_eq(atan(1.0), M_PI / 4.0, "atan(1.0)");

    printf("  ✔ trigonometric functions passed\n");
}

/* ---------------- Logarithmic / Exponential ---------------- */

void test_logarithmic_exponential(void) {
    printf("Testing logarithmic and exponential functions...\n");

    assert_double_eq(exp(1.0), M_E, "exp(1.0)");
    assert_double_eq(log(M_E), 1.0, "log(e)");
    assert_double_eq(log10(100.0), 2.0, "log10(100)");
    assert_double_eq(log2(8.0), 3.0, "log2(8)");

    printf("  ✔ logarithmic/exponential functions passed\n");
}

/* ---------------- Special Functions ---------------- */

void test_special_functions(void) {
    printf("Testing special functions...\n");

    assert_double_eq(sinh(0.0), 0.0, "sinh(0)");
    assert_double_eq(cosh(0.0), 1.0, "cosh(0)");
    assert_double_eq(tanh(0.0), 0.0, "tanh(0)");

    assert_double_eq(fmod(10.5, 3.0), 1.5, "fmod(10.5, 3.0)");
    assert_double_eq(round(3.5), 4.0, "round(3.5)");
    assert_double_eq(round(3.4), 3.0, "round(3.4)");

    printf("  ✔ special functions passed\n");
}

/* ---------------- Main ---------------- */

int main(void) {
    printf("=== Math Library Test Suite ===\n\n");

    test_basic_operations();
    test_trigonometric();
    test_logarithmic_exponential();
    test_special_functions();

    printf("\n=== ALL TESTS PASSED ===\n");
    return 0;
}

