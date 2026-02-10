#include <stdio.h>

void never_called_a() {
    int x = 42;
    printf("this never runs %d\n", x);
}

void never_called_b() {
    int y = 100;
    y = y + 1;
}

int helper(int n) {
    return n * 2;
}

int main() {
    int result = helper(5);
    printf("%d\n", result);
    return 0;
}
