#include <assert.h>
#include <time.h>

int main() {
    struct timespec ts;
    
    // Call clock_gettime and assert success
    int ret = clock_gettime(CLOCK_REALTIME, &ts);
    assert(ret == 0);
    
    // Assert invariants
    assert(ts.tv_nsec >= 0);
    assert(ts.tv_nsec < 1000000000L);
    assert(ts.tv_sec > 0);
    
    // Optional: Call clock_gettime twice back-to-back and assert the second is not earlier
    struct timespec ts2;
    ret = clock_gettime(CLOCK_REALTIME, &ts2);
    assert(ret == 0);
    assert(ts2.tv_sec > ts.tv_sec || (ts2.tv_sec == ts.tv_sec && ts2.tv_nsec >= ts.tv_nsec));
    
    return 0;
}
