#include <stdio.h>
#include <string.h>

static const char *const greetings[] = {"hello", "hej", "hallo", "bonjour"};
int counter = 3;
static char buffer[4096];

static unsigned hash(const char *s)
{
    unsigned h = 5381;
    while (*s) {
        h = h * 33u + (unsigned char)*s++;
    }
    return h;
}

int main(int argc, char **argv)
{
    (void)argv;
    const char *g = greetings[(unsigned)argc % 4u];
    strncpy(buffer, g, sizeof(buffer) - 1);
    printf("%s %u %d\n", buffer, hash(buffer), counter);
    return 0;
}
