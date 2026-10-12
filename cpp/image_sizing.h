#pragma once

#include <cstdint>
#include <optional>
#include <utility>
#include <vector>

// Port of upstream/Sources/LlamaRuntime/ImageSizing.swift.
// transformers' Gemma 4 image processor + Pillow-exact bicubic (a = −0.5).
namespace trowel {

struct Size {
    int height = 0;
    int width = 0;
    bool operator==(const Size &o) const { return height == o.height && width == o.width; }
};

// Scale to fill `budget` soft tokens (each 3×3 patches of 16 px), aspect kept,
// each side floored to a multiple of 48 px. nullopt if both sides round to 0.
std::optional<Size> target_size(int height, int width, int budget);

// Pillow bicubic of packed RGB. `source` is width*height*3 bytes.
std::vector<std::uint8_t> resize_bicubic(const std::uint8_t *source, int width, int height,
                                         int to_width, int to_height);

}  // namespace trowel
