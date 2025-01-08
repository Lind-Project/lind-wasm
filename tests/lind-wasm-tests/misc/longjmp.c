#include <stdio.h>
#include <setjmp.h>

jmp_buf jump_buffer;

void second_function() {
    printf("In second_function: Performing a long jump...\n");
    longjmp(jump_buffer, 42); // Jump back to the point where setjmp was called
}

void first_function() {
    printf("In first_function: Calling second_function...\n");
    second_function(); // Call a function that performs a long jump
    printf("This line will never be printed because of the long jump.\n");
}

int main() {
    int val = setjmp(jump_buffer);

    if (val != 0) {
        // This block runs after longjmp is called
        printf("Back in main: long jump returned with value = %d\n", val);
        return 0;
    }

    printf("In main: Calling first_function...\n");
    first_function();

    printf("This line will also never be printed because of the long jump.\n");
    return 0;
}
