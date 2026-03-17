#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <errno.h>
#include <unistd.h>
#include <assert.h>

int main(void) {
    printf("Testing getsockopt() syscall\n");
    fflush(stdout);
    
    // Test 1: Get default SO_REUSEADDR (should be 0)
    int sock = socket(AF_INET, SOCK_STREAM, 0);
    assert(sock >= 0);
    
    int optval;
    socklen_t optlen = sizeof(optval);
    
    assert(getsockopt(sock, SOL_SOCKET, SO_REUSEADDR, &optval, &optlen) == 0);
    assert(optval == 0);
    
    printf("Test 1 passed: Default SO_REUSEADDR is 0\n");
    fflush(stdout);
    
    // Test 2: Set SO_REUSEADDR to 1, then get it back (round-trip test)
    optval = 1;
    assert(setsockopt(sock, SOL_SOCKET, SO_REUSEADDR, &optval, sizeof(optval)) == 0);
    
    // Reset optval to verify getsockopt actually retrieves the value
    optval = 0;
    optlen = sizeof(optval);
    assert(getsockopt(sock, SOL_SOCKET, SO_REUSEADDR, &optval, &optlen) == 0);
    assert(optval == 1);
    
    printf("Test 2 passed: Set and get SO_REUSEADDR works correctly\n");
    fflush(stdout);
    
    // Test 3: Get SO_TYPE (read-only option, should be SOCK_STREAM)
    int socktype;
    optlen = sizeof(socktype);
    assert(getsockopt(sock, SOL_SOCKET, SO_TYPE, &socktype, &optlen) == 0);
    assert(socktype == SOCK_STREAM);
    
    printf("Test 3 passed: SO_TYPE is SOCK_STREAM\n");
    fflush(stdout);
    
    // Test 4: Get default SO_KEEPALIVE (should be 0)
    int keepalive;
    optlen = sizeof(keepalive);
    assert(getsockopt(sock, SOL_SOCKET, SO_KEEPALIVE, &keepalive, &optlen) == 0);
    assert(keepalive == 0);
    
    printf("Test 4 passed: Default SO_KEEPALIVE is 0\n");
    fflush(stdout);
    
    // Test 5: Set SO_KEEPALIVE to 1, then get it back
    keepalive = 1;
    assert(setsockopt(sock, SOL_SOCKET, SO_KEEPALIVE, &keepalive, sizeof(keepalive)) == 0);
    
    keepalive = 0; // Reset to verify getsockopt works
    optlen = sizeof(keepalive);
    assert(getsockopt(sock, SOL_SOCKET, SO_KEEPALIVE, &keepalive, &optlen) == 0);
    assert(keepalive == 1);
    
    printf("Test 5 passed: Set and get SO_KEEPALIVE works correctly\n");
    fflush(stdout);
    
    // Test 6: Test error case - invalid socket FD
    int invalid_optval;
    optlen = sizeof(invalid_optval);
    errno = 0; // Reset errno before error test
    assert(getsockopt(-1, SOL_SOCKET, SO_REUSEADDR, &invalid_optval, &optlen) < 0);
    assert(errno == EBADF);
    
    printf("Test 6 passed: Invalid FD correctly returns EBADF\n");
    fflush(stdout);
    
    // Test 7: Test invalid option name (ENOPROTOOPT)
    int dummy;
    optlen = sizeof(dummy);
    errno = 0; // Reset errno before error test
    assert(getsockopt(sock, SOL_SOCKET, 9999, &dummy, &optlen) < 0);
    assert(errno == ENOPROTOOPT);
    
    printf("Test 7 passed: Invalid option name correctly returns ENOPROTOOPT\n");
    fflush(stdout);
    
    // Test 8: Test invalid protocol level (EOPNOTSUPP)
    // Note: Invalid protocol level returns EOPNOTSUPP, not ENOPROTOOPT
    optlen = sizeof(dummy);
    errno = 0; // Reset errno before error test
    assert(getsockopt(sock, 9999, SO_REUSEADDR, &dummy, &optlen) < 0);
    assert(errno == EOPNOTSUPP);
    
    printf("Test 8 passed: Invalid protocol level correctly returns EOPNOTSUPP\n");
    fflush(stdout);
    
    // Test 9: Test with UDP socket - SO_TYPE should be SOCK_DGRAM
    int udp_sock = socket(AF_INET, SOCK_DGRAM, 0);
    assert(udp_sock >= 0);
    
    int udp_type;
    optlen = sizeof(udp_type);
    assert(getsockopt(udp_sock, SOL_SOCKET, SO_TYPE, &udp_type, &optlen) == 0);
    assert(udp_type == SOCK_DGRAM);
    
    printf("Test 9 passed: SO_TYPE on UDP socket is SOCK_DGRAM\n");
    fflush(stdout);
    
    // Cleanup
    close(sock);
    close(udp_sock);
    
    printf("All getsockopt() tests passed successfully\n");
    fflush(stdout);
    return EXIT_SUCCESS;
}

