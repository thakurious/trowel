#include "image_sizing.h"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdint>

namespace trowel {
namespace {

constexpr std::int32_t kPrecision = 22;

std::int32_t wrapping_add(std::int32_t a, std::int32_t b) {
    return static_cast<std::int32_t>(static_cast<std::uint32_t>(a) + static_cast<std::uint32_t>(b));
}

std::int32_t wrapping_mul(std::int32_t a, std::int32_t b) {
    return static_cast<std::int32_t>(static_cast<std::uint32_t>(a) * static_cast<std::uint32_t>(b));
}

double filter_kernel(double value) {
    const double x = std::abs(value);
    const double a = -0.5;
    if (x < 1) {
        return ((a + 2) * x - (a + 3)) * x * x + 1;
    }
    if (x < 2) {
        return (((x - 5) * x + 8) * x - 4) * a;
    }
    return 0;
}

struct Coeffs {
    std::vector<int> bounds;
    std::vector<std::int32_t> weights;
    int size = 0;
};

Coeffs coefficients(int in_size, int out_size) {
    const double scale = static_cast<double>(in_size) / static_cast<double>(out_size);
    const double filter_scale = std::max(scale, 1.0);
    const double support = 2 * filter_scale;
    const int size = static_cast<int>(std::ceil(support)) * 2 + 1;
    Coeffs c;
    c.size = size;
    c.bounds.assign(static_cast<std::size_t>(out_size) * 2, 0);
    c.weights.assign(static_cast<std::size_t>(out_size) * static_cast<std::size_t>(size), 0);
    std::vector<double> row(static_cast<std::size_t>(size), 0);
    for (int out = 0; out < out_size; ++out) {
        const double center = (static_cast<double>(out) + 0.5) * scale;
        const int first = std::max(static_cast<int>(center - support + 0.5), 0);
        const int count = std::min(static_cast<int>(center + support + 0.5), in_size) - first;
        double total = 0;
        for (int x = 0; x < size; ++x) {
            row[static_cast<std::size_t>(x)] =
                x < count ? filter_kernel((static_cast<double>(x + first) - center + 0.5) / filter_scale)
                          : 0;
            total += row[static_cast<std::size_t>(x)];
        }
        for (int x = 0; x < size; ++x) {
            const double weight = (x < count && total != 0) ? row[static_cast<std::size_t>(x)] / total
                                                            : row[static_cast<std::size_t>(x)];
            c.weights[static_cast<std::size_t>(out * size + x)] = static_cast<std::int32_t>(
                weight * static_cast<double>(1 << kPrecision) + (weight < 0 ? -0.5 : 0.5));
        }
        c.bounds[static_cast<std::size_t>(out * 2)] = first;
        c.bounds[static_cast<std::size_t>(out * 2 + 1)] = count;
    }
    return c;
}

std::uint8_t clip(std::int32_t value) {
    const std::int32_t v = value >> kPrecision;
    if (v < 0) {
        return 0;
    }
    if (v > 255) {
        return 255;
    }
    return static_cast<std::uint8_t>(v);
}

std::vector<std::uint8_t> horizontal(const std::uint8_t *src, int in_width, int rows, int out_width) {
    const Coeffs c = coefficients(in_width, out_width);
    std::vector<std::uint8_t> out(static_cast<std::size_t>(out_width) * static_cast<std::size_t>(rows) * 3);
    for (int y = 0; y < rows; ++y) {
        for (int x = 0; x < out_width; ++x) {
            const std::uint8_t *p =
                src + (static_cast<std::size_t>(y) * static_cast<std::size_t>(in_width) +
                       static_cast<std::size_t>(c.bounds[static_cast<std::size_t>(x * 2)])) *
                          3;
            std::int32_t r = 1 << (kPrecision - 1);
            std::int32_t g = r;
            std::int32_t b = r;
            const int count = c.bounds[static_cast<std::size_t>(x * 2 + 1)];
            for (int k = 0; k < count; ++k) {
                const std::int32_t w = c.weights[static_cast<std::size_t>(x * c.size + k)];
                r = wrapping_add(r, wrapping_mul(static_cast<std::int32_t>(p[0]), w));
                g = wrapping_add(g, wrapping_mul(static_cast<std::int32_t>(p[1]), w));
                b = wrapping_add(b, wrapping_mul(static_cast<std::int32_t>(p[2]), w));
                p += 3;
            }
            const std::size_t o =
                (static_cast<std::size_t>(y) * static_cast<std::size_t>(out_width) + static_cast<std::size_t>(x)) * 3;
            out[o] = clip(r);
            out[o + 1] = clip(g);
            out[o + 2] = clip(b);
        }
    }
    return out;
}

std::vector<std::uint8_t> vertical(const std::uint8_t *src, int in_height, int row_width, int out_height) {
    const Coeffs c = coefficients(in_height, out_height);
    const int stride = row_width * 3;
    std::vector<std::uint8_t> out(static_cast<std::size_t>(stride) * static_cast<std::size_t>(out_height));
    std::vector<std::int32_t> sums(static_cast<std::size_t>(stride));
    for (int y = 0; y < out_height; ++y) {
        const std::int32_t bias = 1 << (kPrecision - 1);
        for (int i = 0; i < stride; ++i) {
            sums[static_cast<std::size_t>(i)] = bias;
        }
        const int count = c.bounds[static_cast<std::size_t>(y * 2 + 1)];
        for (int k = 0; k < count; ++k) {
            const std::uint8_t *row =
                src + (c.bounds[static_cast<std::size_t>(y * 2)] + k) * stride;
            const std::int32_t w = c.weights[static_cast<std::size_t>(y * c.size + k)];
            for (int i = 0; i < stride; ++i) {
                sums[static_cast<std::size_t>(i)] = wrapping_add(
                    sums[static_cast<std::size_t>(i)],
                    wrapping_mul(static_cast<std::int32_t>(row[i]), w));
            }
        }
        for (int i = 0; i < stride; ++i) {
            out[static_cast<std::size_t>(y) * static_cast<std::size_t>(stride) + static_cast<std::size_t>(i)] =
                clip(sums[static_cast<std::size_t>(i)]);
        }
    }
    return out;
}

}  // namespace

std::optional<Size> target_size(int height, int width, int budget) {
    // upstream/Sources/LlamaRuntime/ImageSizing.swift:9
    constexpr int patch = 16;
    constexpr int pooling = 3;
    const int max_patches = budget * pooling * pooling;
    const double factor = std::sqrt(static_cast<double>(max_patches * patch * patch) /
                                    static_cast<double>(height * width));
    const int side = pooling * patch;
    auto floored = [&](int dim) {
        return static_cast<int>(std::floor(factor * static_cast<double>(dim) / static_cast<double>(side))) * side;
    };
    int target_height = floored(height);
    int target_width = floored(width);
    if (target_height == 0 && target_width == 0) {
        return std::nullopt;
    }
    const int max_side = (max_patches / (pooling * pooling)) * side;
    if (target_height == 0) {
        target_height = side;
        target_width = std::min(width / height * side, max_side);
    } else if (target_width == 0) {
        target_width = side;
        target_height = std::min(height / width * side, max_side);
    }
    return Size{target_height, target_width};
}

std::vector<std::uint8_t> resize_bicubic(const std::uint8_t *source, int width, int height,
                                         int to_width, int to_height) {
    const bool hx = to_width != width;
    const bool hy = to_height != height;
    if (hx && hy) {
        auto pass = horizontal(source, width, height, to_width);
        return vertical(pass.data(), height, to_width, to_height);
    }
    if (hx) {
        return horizontal(source, width, height, to_width);
    }
    if (hy) {
        return vertical(source, height, width, to_height);
    }
    return std::vector<std::uint8_t>(source, source + static_cast<std::size_t>(width) * height * 3);
}

}  // namespace trowel
