#include <arpa/inet.h>
#include <assert.h>
#include <ifaddrs.h>
#include <netinet/in.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
  struct ifaddrs *ifaddr, *ifa;

  assert(getifaddrs(&ifaddr) == 0);

  for (ifa = ifaddr; ifa != NULL; ifa = ifa->ifa_next) {
    assert(ifa->ifa_addr != NULL);
  }

  printf("getifaddrs ok\n");
  fflush(stdout);
  freeifaddrs(ifaddr);
  exit(EXIT_SUCCESS);
}
