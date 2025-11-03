#include <stdint.h>
#include <stddef.h>

/**
 * Add two arrays of limbs of the same length and store the result in a third
 * array.
 * @param res Pointer to the result array.
 * @param s1 Pointer to the first source array.
 * @param s2 Pointer to the second source array.
 * @param size Number of elements (limbs) in each array.
 * @return Carry-out from the most significant limb addition.
 */
uint32_t
__mpn_add_n (uint32_t *res, const uint32_t *s1, const uint32_t *s2,
	     size_t size)
{
  uint32_t carry = 0;
  for (size_t i = 0; i < size; i++)
    {
      uint64_t sum = (uint64_t) s1[i] + s2[i] + carry;
      res[i] = (uint32_t) sum;	      // Store lower 32 bits of the result
      carry = (uint32_t) (sum >> 32); // Propagate upper 32 bits as carry to
				      // next iteration
    }
  return carry; // Return the carry-out from the most significant limb addition
}
