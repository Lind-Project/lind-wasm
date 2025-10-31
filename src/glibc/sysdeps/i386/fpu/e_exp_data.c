/* Not needed.  */

// lind-wasm: added wrapper function for wasm compilation
double exp_data(double x) {
  return __ieee754_exp_data(x);
}
