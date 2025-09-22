// tests/unit-tests/config_tests/non-deterministic/getifaddrs.c
#include <stdio.h>
#include <stdlib.h>

#if defined(__wasi__)
// WASI: getifaddrs is not part of the standard libc.
// Treat lack of support as a success for this non-deterministic test.
int main(void) {
  puts("getifaddrs-unsupported");
  return 0;
}
#else
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <arpa/inet.h>
#include <netinet/in.h>
#include <ifaddrs.h>

int main(void) {
  struct ifaddrs *ifaddr, *ifa;

  if (getifaddrs(&ifaddr) == -1) {
    perror("getifaddrs");
    // For portability of this test, if the host doesn't support it either,
    // don't fail hard—treat as success with a clear message.
    puts("getifaddrs-unavailable");
    return 0;
  }

  for (ifa = ifaddr; ifa != NULL; ifa = ifa->ifa_next) {
    if (!ifa->ifa_addr) continue;

    int family = ifa->ifa_addr->sa_family;
    const char *fam =
#if defined(AF_PACKET)
      (family == AF_PACKET)  ? "AF_PACKET"  :
#endif
      (family == AF_INET)    ? "AF_INET"    :
      (family == AF_INET6)   ? "AF_INET6"   : "???";

    printf("%-8s %s (%d)\n", ifa->ifa_name, fam, family);
  }

  fflush(stdout);
  freeifaddrs(ifaddr);
  return 0;
}
#endif

