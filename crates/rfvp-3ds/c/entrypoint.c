#include "libctru_backend.h"

int main(int argc, char **argv) {
    int status = rfvp_3ds_platform_init(argc, argv);
    if (status != RFVP_THREE_DS_OK) {
        return status;
    }

    RawThreeDsHost host;
    status = rfvp_3ds_make_raw_host(&host);
    if (status == RFVP_THREE_DS_OK) {
        status = rfvp_3ds_app_main(&host);
    }

    rfvp_3ds_platform_fini();
    return status;
}
