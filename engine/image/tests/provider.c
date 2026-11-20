#include <stdint.h>
typedef int (*check_fn)(uintptr_t);
int check_on_caller_thread(check_fn check, uintptr_t budget) { return check(budget); }
