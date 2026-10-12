#include "embedder.h"

#include <nlohmann/json.hpp>

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <iostream>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <vector>

using json = nlohmann::json;

namespace {

void emit(const json &obj) {
    std::cout << obj.dump() << '\n';
    std::cout.flush();
}

std::string b64_encode(const std::vector<float> &floats) {
    static const char tbl[] =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    std::string out;
    const auto *p = reinterpret_cast<const unsigned char *>(floats.data());
    const std::size_t n = floats.size() * sizeof(float);
    out.reserve(((n + 2) / 3) * 4);
    std::size_t i = 0;
    while (i + 2 < n) {
        const unsigned v = (static_cast<unsigned>(p[i]) << 16) | (static_cast<unsigned>(p[i + 1]) << 8) | p[i + 2];
        out.push_back(tbl[(v >> 18) & 63]);
        out.push_back(tbl[(v >> 12) & 63]);
        out.push_back(tbl[(v >> 6) & 63]);
        out.push_back(tbl[v & 63]);
        i += 3;
    }
    if (i < n) {
        unsigned v = static_cast<unsigned>(p[i]) << 16;
        if (i + 1 < n) {
            v |= static_cast<unsigned>(p[i + 1]) << 8;
        }
        out.push_back(tbl[(v >> 18) & 63]);
        out.push_back(tbl[(v >> 12) & 63]);
        out.push_back(i + 1 < n ? tbl[(v >> 6) & 63] : '=');
        out.push_back('=');
    }
    return out;
}

json info_fields(const trowel::WorkerInfo &info) {
    return json{{"model", info.model},
                {"revision", info.revision},
                {"runtime", info.runtime},
                {"dtype", info.dtype},
                {"dim", info.dim},
                {"text_only", info.text_only},
                {"load_seconds", info.load_seconds}};
}

std::vector<std::string> inputs_of(const json &req) {
    std::vector<std::string> inputs;
    if (!req.contains("inputs") || !req["inputs"].is_array()) {
        return inputs;
    }
    for (const auto &item : req["inputs"]) {
        inputs.push_back(item.is_string() ? item.get<std::string>() : "");
    }
    return inputs;
}

json id_of(const json &req) {
    if (req.contains("id")) {
        return req["id"];
    }
    return nullptr;
}

}  // namespace

int main(int argc, char **argv) {
    bool text_only = false;
    std::string backend = "vulkan";
    for (int i = 1; i < argc; ++i) {
        if (std::strcmp(argv[i], "--text-only") == 0) {
            text_only = true;
        } else if (std::strcmp(argv[i], "--backend") == 0 && i + 1 < argc) {
            backend = argv[++i];
        } else if (std::strncmp(argv[i], "--backend=", 10) == 0) {
            backend = argv[i] + 10;
        } else if (std::strcmp(argv[i], "--help") == 0 || std::strcmp(argv[i], "-h") == 0) {
            std::fprintf(stderr,
                         "trowel-worker [--backend vulkan|cpu] [--text-only]\n"
                         "JSON-lines on stdin: info | text | image | audio | quit\n");
            return 0;
        }
    }
    if (backend != "vulkan" && backend != "cpu") {
        emit({{"ready", false}, {"error", "backend must be vulkan or cpu"}});
        return 1;
    }
#ifdef TROWEL_VULKAN
    const bool built_vulkan = true;
#else
    const bool built_vulkan = false;
#endif
    const bool use_gpu = backend == "vulkan";
    if (use_gpu && !built_vulkan) {
        emit({{"ready", false},
              {"error", "this trowel-worker was built CPU-only; rebuild with TROWEL_VULKAN=ON"}});
        return 1;
    }
    if (!use_gpu && built_vulkan) {
        // A Vulkan-linked binary can still run on CPU by not offloading layers.
        std::fprintf(stderr, "[llama.cpp worker] --backend cpu on a Vulkan build: n_gpu_layers=0\n");
    }

    std::unique_ptr<trowel::Embedder> embedder;
    try {
        auto models = trowel::locate_models();
        if (!text_only && !models.media) {
            throw std::runtime_error("mmproj-embeddinggemma-2-Q8_0.gguf isn't downloaded yet");
        }
        embedder = std::make_unique<trowel::Embedder>(models.text, models.media, text_only, use_gpu);
    } catch (const std::exception &e) {
        emit({{"ready", false}, {"error", e.what()}});
        return 1;
    }

    const json fields = info_fields(embedder->info());
    emit({{"ready", true}, {"info", fields}});

    std::string line;
    while (std::getline(std::cin, line)) {
        if (line.empty()) {
            continue;
        }
        json req;
        try {
            req = json::parse(line);
        } catch (...) {
            continue;
        }
        if (!req.is_object()) {
            continue;
        }
        const json id = id_of(req);
        const auto inputs = inputs_of(req);
        try {
            const std::string op = req.value("op", "");
            if (op == "quit") {
                emit({{"id", id}, {"ok", true}});
                embedder->close();
                return 0;
            }
            if (op == "info") {
                emit({{"id", id}, {"ok", true}, {"info", fields}});
                continue;
            }
            std::vector<trowel::Embedding> embeddings;
            if (op == "text") {
                auto vecs = embedder->embed_texts(inputs);
                embeddings.reserve(vecs.size());
                for (auto &v : vecs) {
                    embeddings.push_back(trowel::Embedding{std::move(v), std::nullopt});
                }
            } else if (op == "image") {
                const int budget = req.value("budget", 280);
                embeddings = embedder->embed_images(inputs, budget);
            } else if (op == "audio") {
                embeddings = embedder->embed_audio(inputs);
            } else {
                throw std::runtime_error("unknown op " + (op.empty() ? std::string("(none)") : op));
            }
            const int dim = embedder->info().dim;
            std::vector<float> floats;
            floats.reserve(inputs.size() * static_cast<std::size_t>(dim));
            json errors = json::array();
            for (const auto &e : embeddings) {
                if (e.vector) {
                    floats.insert(floats.end(), e.vector->begin(), e.vector->end());
                    errors.push_back(nullptr);
                } else {
                    floats.insert(floats.end(), static_cast<std::size_t>(dim), 0.f);
                    errors.push_back(e.error ? json(*e.error) : json(nullptr));
                }
            }
            emit({{"id", id},
                  {"ok", true},
                  {"dim", dim},
                  {"count", static_cast<int>(inputs.size())},
                  {"vectors", b64_encode(floats)},
                  {"errors", errors}});
        } catch (const std::exception &e) {
            std::string msg = e.what();
            if (msg.size() > 500) {
                msg.resize(500);
            }
            emit({{"id", id}, {"ok", false}, {"error", msg}});
        }
    }
    embedder->close();
    return 0;
}
