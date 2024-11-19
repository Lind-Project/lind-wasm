#include <stdint.h>
#include <stddef.h>

/**
 * Multiply a limb vector with a limb and add the result to a second limb vector.
 * @param res_ptr Result vector to which to add the products.
 * @param s1_ptr Source vector of limbs to be multiplied.
 * @param size Number of elements in the vectors.
 * @param s2_limb Limb multiplier.
 * @return Carry-out from the most significant limb addition.
 */
uint32_t __mpn_addmul_1(uint32_t *res_ptr, const uint32_t *s1_ptr, size_t size, uint32_t s2_limb) {
    uint32_t carry = 0;

    for (size_t i = 0; i < size; i++) {
        uint64_t product = (uint64_t) s1_ptr[i] * (uint64_t) s2_limb;
        uint32_t low_product = (uint32_t) product;
        uint32_t high_product = (uint32_t) (product >> 32);

        // Add the lower product and the carry to the result
        uint32_t sum = res_ptr[i] + low_product + carry;
        
        // Determine if there was a carry from the addition
        if (sum < res_ptr[i] || sum < low_product) carry = 1;
        else carry = 0;

        // Add the high product and any additional carry from the lower addition
        carry += high_product;

        // Store the result
        res_ptr[i] = sum;
    }

    // Return the final carry value, which may need to be added to the next limb in a larger operation
    return carry;
}

