#include <unistd.h>
#include <string.h>

int main() {
  const char* message = "Hello world from Coulson's WASM\n";
  int len = strlen(message);
  write(1, message, len);  // simply write to stdout
  return 0;
}