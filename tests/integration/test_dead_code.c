#include <stdio.h>

int main() {
    int x = 1;
    if (x > 100) {
        int dead_a = 10;
        int dead_b = 20;
        printf("never reached %d %d\n", dead_a, dead_b);
    }
    printf("%d\n", x);
    return 0;
}
