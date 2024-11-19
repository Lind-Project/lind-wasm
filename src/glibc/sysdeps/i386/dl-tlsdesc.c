#include <stdint.h>
#include <stddef.h>

int _dl_tlsdesc_return(void) {
    return 0;
}

int _dl_tlsdesc_undefweak(void) {

    return 0;
}

int _dl_tlsdesc_dynamic_xsavec(void) {
  return 0;
}

int _dl_tlsdesc_dynamic_fxsave(void) {
  return 0;
}

int _dl_tlsdesc_dynamic_xsave(void) {
  return 0;
}

int _dl_tlsdesc_dynamic_fnsave(void) {
  return 0;
}
