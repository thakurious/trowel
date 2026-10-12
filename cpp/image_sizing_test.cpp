#include "image_sizing.h"

#include <cstdio>
#include <vector>

using trowel::resize_bicubic;
using trowel::Size;
using trowel::target_size;

static int failures = 0;

static void expect_size(const char *name, std::optional<Size> got, Size want) {
    if (!got || !(*got == want)) {
        std::fprintf(stderr, "FAIL %s: got %s want %dx%d\n", name,
                     got ? "size" : "nullopt", want.height, want.width);
        if (got) {
            std::fprintf(stderr, "      actual %dx%d\n", got->height, got->width);
        }
        ++failures;
        return;
    }
    std::printf("ok %s\n", name);
}

static void expect_bytes(const char *name, const std::vector<std::uint8_t> &got,
                         const std::vector<std::uint8_t> &want) {
    if (got != want) {
        std::fprintf(stderr, "FAIL %s: %zu vs %zu bytes\n", name, got.size(), want.size());
        ++failures;
        return;
    }
    std::printf("ok %s\n", name);
}

int main() {
    // upstream/Tests/LlamaRuntimeTests/ImageSizingTests.swift
    expect_size("shot-280", target_size(900, 1440, 280), Size{624, 1008});
    expect_size("shot-140", target_size(900, 1440, 140), Size{432, 672});
    expect_size("shot-70", target_size(900, 1440, 70), Size{288, 480});
    expect_size("photo-280", target_size(768, 768, 280), Size{768, 768});
    expect_size("photo-140", target_size(768, 768, 140), Size{528, 528});
    expect_size("grow-140", target_size(300, 300, 140), Size{528, 528});
    expect_size("thin-140", target_size(10, 4000, 140), Size{48, 6720});

    const std::vector<std::uint8_t> source = {
        0,   115, 230, 3,   118, 233, 6,   121, 236, 9,   124, 239, 12,  127, 242, 15,  130, 245, 18,
        133, 248, 77,  192, 51,  80,  195, 54,  83,  198, 57,  86,  201, 60,  89,  204, 63,  92,  207,
        66,  95,  210, 69,  154, 13,  128, 157, 16,  131, 160, 19,  134, 163, 22,  137, 166, 25,  140,
        169, 28,  143, 172, 31,  146, 231, 90,  205, 234, 93,  208, 237, 96,  211, 240, 99,  214, 243,
        102, 217, 246, 105, 220, 249, 108, 223, 52,  167, 26,  55,  170, 29,  58,  173, 32,  61,  176,
        35,  64,  179, 38,  67,  182, 41,  70,  185, 44};

    const std::vector<std::uint8_t> pil_shrink = {28,  151, 152, 33,  156, 157, 39,  162, 163, 44,  167, 168,
                                                  165, 68,  129, 170, 73,  134, 176, 79,  140, 181, 84,  145,
                                                  132, 133, 106, 137, 138, 111, 143, 144, 117, 148, 149, 122};
    expect_bytes("shrink", resize_bicubic(source.data(), 7, 5, 4, 3), pil_shrink);

    const std::vector<std::uint8_t> pil_grow = {
        0,   112, 237, 0,   113, 238, 1,   116, 241, 3,   118, 243, 5,   120, 245, 7,   122, 247, 9,   124, 249,
        11,  126, 251, 14,  129, 254, 15,  130, 255, 55,  187, 85,  56,  188, 86,  59,  191, 89,  61,  193, 91,
        63,  195, 93,  65,  197, 95,  67,  199, 97,  69,  201, 99,  72,  204, 102, 73,  205, 103, 122, 82,  83,
        123, 83,  84,  126, 86,  87,  128, 88,  89,  130, 90,  91,  132, 92,  93,  134, 94,  95,  136, 96,  97,
        139, 99,  100, 140, 100, 101, 199, 27,  173, 200, 28,  174, 203, 31,  177, 205, 33,  179, 207, 35,  181,
        209, 37,  183, 211, 39,  185, 213, 41,  187, 216, 44,  190, 217, 45,  191, 197, 112, 171, 198, 113, 172,
        201, 116, 175, 203, 118, 177, 205, 120, 179, 207, 122, 181, 209, 124, 183, 211, 126, 185, 214, 129, 188,
        215, 130, 189, 45,  170, 19,  46,  171, 20,  49,  174, 23,  51,  176, 25,  53,  178, 27,  55,  180, 29,
        57,  182, 31,  59,  184, 33,  62,  187, 36,  63,  188, 37};
    expect_bytes("grow", resize_bicubic(source.data(), 7, 5, 10, 6), pil_grow);

    const std::vector<std::uint8_t> pil_one_axis = {74,  120, 137, 77,  123, 140, 80,  126, 143, 83,  129, 146, 86,  132,
                                                   149, 89,  135, 152, 92,  138, 155, 153, 100, 119, 156, 103, 122, 159,
                                                   106, 125, 162, 109, 128, 165, 112, 131, 168, 115, 134, 171, 118, 137};
    expect_bytes("one-axis", resize_bicubic(source.data(), 7, 5, 7, 2), pil_one_axis);
    expect_bytes("identity", resize_bicubic(source.data(), 7, 5, 7, 5), source);

    if (failures) {
        std::fprintf(stderr, "%d image-sizing test(s) failed\n", failures);
        return 1;
    }
    std::printf("image-sizing: all passed\n");
    return 0;
}
