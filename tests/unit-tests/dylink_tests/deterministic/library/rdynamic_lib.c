// libplugin.c
#include <stdio.h>

// Declare the function from the main executable
extern void hello_from_main(void);

void plugin_entry(void) {
    printf("Hello from plugin!\n");

    // Call back into the main executable
    hello_from_main();
}
