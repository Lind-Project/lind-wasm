#include <algorithm>
#include <vector>
#include <iostream>

int main() {
    std::vector<int> v = {3, 1, 2};
    std::sort(v.begin(), v.end());

    for (int each : v){
        std::cout<< each<<' ' ;
    }
    std::cout<<std::endl;
    return 0;
}
