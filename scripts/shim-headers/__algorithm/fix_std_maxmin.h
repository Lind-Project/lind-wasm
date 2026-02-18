// fix_std_maxmin.h (issue #245)
// Avoid inconsistent two-parameter types for std::max/std::min so template _Tp deduction succeeds.
#pragma once

#include <algorithm>
#include <type_traits>

namespace std {

template <class _Tp, class _Up>
constexpr typename std::common_type<_Tp, _Up>::type
max(const _Tp& __a, const _Up& __b) {
  return (__a < __b) ? __b : __a;
}

template <class _Tp, class _Up>
constexpr typename std::common_type<_Tp, _Up>::type
min(const _Tp& __a, const _Up& __b) {
  return (__b < __a) ? __b : __a;
}

}
