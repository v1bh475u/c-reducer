#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef int MyInt;
typedef char MyChar;

void dead_helper() {
    int z = 99;
    printf("never called %d\n", z);
}

int compute(int n) {
    return n + 1;
}

int main() {
    int x;
    int y;
    int unused_var = 100;
    x = compute(5);
    y = compute(10);
    printf("%d %d\n", x, y);
    return 0;
}
