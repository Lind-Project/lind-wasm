#include <stdio.h>

void __attribute__ ((constructor)) _init (void);
void __attribute__ ((destructor)) _fini (void);

void
preinit_function (void)
{
  // Placeholder for actual initialization code
  printf ("Pre-initialization function called.\n");
}

void
_init (void)
{
  // Perform initialization tasks
  printf ("Initialization function called.\n");
  preinit_function (); // Call the pre-initialization function
}

void
_fini (void)
{
  // Perform cleanup tasks
  printf ("Finalization function called.\n");
}
