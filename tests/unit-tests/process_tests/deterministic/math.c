#include <stdio.h>
#include <math.h>
#include <assert.h>

void test_basic_operations() {
    printf("Testing basic operations...\n");
    
    // Test sqrt
    double sqrt_result = sqrt(16.0);
    assert(sqrt_result == 4.0);
    printf("  sqrt(16.0) = %.2f ✓\n", sqrt_result);
    
    // Test pow
    double pow_result = pow(2.0, 3.0);
    assert(pow_result == 8.0);
    printf("  pow(2.0, 3.0) = %.2f ✓\n", pow_result);
    
    // Test absolute value
    double abs_result = fabs(-5.5);
    assert(abs_result == 5.5);
    printf("  fabs(-5.5) = %.2f ✓\n", abs_result);
    
    // Test ceil and floor
    assert(ceil(3.2) == 4.0);
    assert(floor(3.8) == 3.0);
    printf("  ceil(3.2) = %.2f ✓\n", ceil(3.2));
    printf("  floor(3.8) = %.2f ✓\n", floor(3.8));
}

void test_trigonometric() {
    printf("\nTesting trigonometric functions...\n");
    
    // Test sin, cos, tan
    double angle = M_PI / 4;  // 45 degrees in radians
    printf("  sin(π/4) = %.6f\n", sin(angle));
    printf("  cos(π/4) = %.6f\n", cos(angle));
    printf("  tan(π/4) = %.6f\n", tan(angle));
    
    // Test inverse functions
    printf("  asin(0.5) = %.6f\n", asin(0.5));
    printf("  acos(0.5) = %.6f\n", acos(0.5));
    printf("  atan(1.0) = %.6f\n", atan(1.0));
}

void test_logarithmic_exponential() {
    printf("\nTesting logarithmic and exponential functions...\n");
    
    // Test exp
    double exp_result = exp(1.0);
    printf("  exp(1.0) = %.6f (should be ~e = 2.718282)\n", exp_result);
    
    // Test log (natural logarithm)
    double log_result = log(M_E);
    printf("  log(e) = %.6f (should be 1.0)\n", log_result);
    
    // Test log10
    double log10_result = log10(100.0);
    assert(log10_result == 2.0);
    printf("  log10(100.0) = %.2f ✓\n", log10_result);
    
    // Test log2
    double log2_result = log2(8.0);
    assert(log2_result == 3.0);
    printf("  log2(8.0) = %.2f ✓\n", log2_result);
}

void test_special_functions() {
    printf("\nTesting special functions...\n");
    
    // Test hyperbolic functions
    printf("  sinh(1.0) = %.6f\n", sinh(1.0));
    printf("  cosh(1.0) = %.6f\n", cosh(1.0));
    printf("  tanh(1.0) = %.6f\n", tanh(1.0));
    
    // Test fmod (floating point remainder)
    double fmod_result = fmod(10.5, 3.0);
    printf("  fmod(10.5, 3.0) = %.2f\n", fmod_result);
    
    // Test round
    printf("  round(3.5) = %.1f\n", round(3.5));
    printf("  round(3.4) = %.1f\n", round(3.4));
}

int main() {
    printf("=== Math Library Test Suite ===\n\n");
    
    test_basic_operations();
    test_trigonometric();
    test_logarithmic_exponential();
    test_special_functions();
    
    printf("\n=== All tests passed! ===\n");
    return 0;
}
