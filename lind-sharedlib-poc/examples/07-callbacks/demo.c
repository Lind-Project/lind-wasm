/* Unmodified native caller. Links against libcallbacks.so and calls apply_cb /
 * report_name with no idea their bodies run inside the sandbox and call BACK out
 * to host handlers. Same drop-in consumer shape as the other examples. */
#include <stdio.h>

int apply_cb(int x);
int report_name(void);

int main(void) {
    printf("apply_cb(21)  = %d   (expect 43: host doubles 21 -> 42, guest +1)\n", apply_cb(21));
    printf("report_name() = %d   (expect 16: host read the guest string's length)\n", report_name());
    return 0;
}
