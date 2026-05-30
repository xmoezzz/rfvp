#include "wut_backend.h"

int main(int argc, char **argv) {
    int status = rfvp_wiiu_platform_init(argc, argv);
    if (status != RFVP_WIIU_OK) {
        return status;
    }

    status = rfvp_wiiu_app_main();

    rfvp_wiiu_platform_fini();
    return status;
}
