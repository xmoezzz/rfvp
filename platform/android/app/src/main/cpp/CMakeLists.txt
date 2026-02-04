cmake_minimum_required(VERSION 3.22.1)

project(rfvp_jni)

add_library(rfvp_jni SHARED
        rfvp_jni.cpp
)

find_library(android-lib android)
find_library(log-lib log)
find_library(dl-lib dl)

target_compile_features(rfvp_jni PRIVATE cxx_std_17)
target_link_libraries(rfvp_jni
        ${android-lib}
        ${log-lib}
        ${dl-lib}
)
