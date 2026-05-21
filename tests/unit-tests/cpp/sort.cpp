#include <algorithm>
#include <iostream>
#include <vector>

int main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    std::vector<int> v = {3, 1, 2};
    std::sort(v.begin(), v.end());

    if (v != std::vector<int>{1, 2, 3}) {
        std::cout << "LIBCPP_SORT_FAIL" << std::endl;
        return 1;
    }

    std::cout << "LIBCPP_SORT_OK 1 2 3" << std::endl;
    return 0;
}
