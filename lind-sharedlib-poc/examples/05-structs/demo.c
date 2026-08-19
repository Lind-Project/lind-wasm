/* A plain native program. It links libpersondemo.so and passes a struct by pointer,
 * unaware the work happens inside the wasm sandbox. The host struct uses the native
 * (LP64) layout; the stub reads its fields and the engine repacks them into the
 * guest's (ILP32) layout, copying the `name` string into guest memory separately. */
#include <stdio.h>
#include <stddef.h>

struct Person {
    int   id;
    long  score;
    char *name;
};

int    person_id(const struct Person *p);       /* int field            */
size_t person_score(const struct Person *p);     /* long field (8B -> 4B) */
size_t person_namelen(const struct Person *p);   /* char* field (nested)  */

int main(void) {
    struct Person p = { .id = 42, .score = 1000, .name = "Ada Lovelace" };

    printf("person_id      -> %d\n", person_id(&p));
    printf("person_score   -> %zu\n", person_score(&p));
    printf("person_namelen -> %zu\n", person_namelen(&p));

    return 0;
}
