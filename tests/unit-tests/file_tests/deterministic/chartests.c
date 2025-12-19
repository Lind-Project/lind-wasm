#include <assert.h>
#include <ctype.h>
#include <stdio.h>

#define UC(c) ((unsigned char)(c))

int main(void) {
    /* Letters */
    assert(isalpha(UC('A')));
    assert(isupper(UC('A')));
    assert(islower(UC('a')));
    assert(isalnum(UC('Z')));
    assert(isgraph(UC('m')));
    assert(isprint(UC('m')));
    assert(!isdigit(UC('m')));
    printf("PASS: alphabetic character tests\n");

    /* Digits */
    assert(isdigit(UC('0')));
    assert(isalnum(UC('9')));
    assert(isxdigit(UC('9')));
    assert(isprint(UC('9')));
    assert(isgraph(UC('9')));
    assert(!isalpha(UC('9')));
    printf("PASS: digit character tests\n");

    /* Hex digits */
    assert(isxdigit(UC('a')));
    assert(isxdigit(UC('F')));
    assert(!isxdigit(UC('g')));
    printf("PASS: hexadecimal digit tests\n");

    /* Whitespace */
    assert(isspace(UC(' ')));
    assert(isblank(UC(' ')));
    assert(isspace(UC('\n')));
    assert(!isblank(UC('\n')));
    printf("PASS: whitespace and blank tests\n");

    /* Control characters */
    assert(iscntrl(UC('\n')));
    assert(iscntrl(UC('\t')));
    assert(!isprint(UC('\n')));
    assert(!isgraph(UC('\n')));
    printf("PASS: control character tests\n");

    /* Punctuation */
    assert(ispunct(UC('!')));
    assert(ispunct(UC(',')));
    assert(isprint(UC('!')));
    assert(isgraph(UC('!')));
    assert(!isalnum(UC('!')));
    printf("PASS: punctuation tests\n");

    /* Sanity negatives */
    assert(!isalpha(UC(' ')));
    assert(!isdigit(UC('A')));
    assert(!ispunct(UC('0')));
    printf("PASS: negative sanity checks\n");

    printf("\nALL CTYPE TESTS PASSED\n");
    return 0;
}

