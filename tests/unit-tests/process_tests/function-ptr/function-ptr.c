#include <stdio.h>
#include <unistd.h>
#include <string.h>

// Define a function pointer type for the function that will do the write syscall
typedef void (*write_message_func_ptr)(const char *);

// Function that performs the write syscall
void do_write(const char *message) {
    size_t length = strlen(message);
    ssize_t bytes_written = write(STDOUT_FILENO, message, length);

    // Check if the write was successful
    if (bytes_written != length) {
        perror("write");
    }
}

// Function that uses a function pointer to call the do_write function
void call_write_function(write_message_func_ptr write_func, const char *message) {
    write_func(message);
}

int main() {
    // Define the message to be written
    const char *message = "Hello, World!\n";

    // Define a function pointer and assign it to the do_write function
    write_message_func_ptr write_func = do_write;

    // Call the function using the function pointer
    call_write_function(write_func, message);

    // Return success
    return 0;
}

