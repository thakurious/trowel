#pragma once

#include "llama.h"
#include "mtmd.h"
#include "mtmd-helper.h"

#include <optional>
#include <string>
#include <utility>
#include <vector>

// Port of upstream/Sources/LlamaRuntime/LlamaEmbedder.swift.
namespace trowel {

struct WorkerInfo {
    std::string model = "google/embeddinggemma-2";
    std::string revision = "bfcd298762cc34d0357ece5ebdd31791a3a374d8";
    std::string runtime = "llama.cpp-vectors1";
    std::string dtype = "text-q8_0+mmproj-q8_0";
    int dim = 768;
    bool text_only = false;
    double load_seconds = 0;
};

struct Embedding {
    std::optional<std::vector<float>> vector;
    std::optional<std::string> error;
};

class Embedder {
public:
    static constexpr int kVectorVersion = 1;
    static constexpr int kMaxSequences = 16;
    static constexpr const char *kBuildTag = "b11461";

    Embedder(const std::string &text_path, const std::optional<std::string> &media_path, bool text_only,
             bool use_gpu);
    ~Embedder();

    Embedder(const Embedder &) = delete;
    Embedder &operator=(const Embedder &) = delete;

    const WorkerInfo &info() const { return info_; }

    std::vector<std::vector<float>> embed_texts(const std::vector<std::string> &texts);
    std::vector<Embedding> embed_images(const std::vector<std::string> &paths, int budget);
    std::vector<Embedding> embed_audio(const std::vector<std::string> &paths);

    void close();

private:
    std::vector<llama_token> tokenize(const std::string &text) const;
    void begin();
    void reset();
    void add_token(llama_token token);
    void add_rows(float *rows, int count);
    void place(int32_t index);
    std::vector<std::vector<float>> run();
    std::vector<float> embed_media(const std::string &path, bool image, int budget);
    mtmd_bitmap *load(const std::string &path, bool image, int budget);

    WorkerInfo info_;
    llama_model *model_ = nullptr;
    llama_context *context_ = nullptr;
    mtmd_context *media_ = nullptr;
    llama_batch_ext *batch_ = nullptr;
    const llama_vocab *vocab_ = nullptr;
    int embedding_size_ = 0;
    int row_size_ = 0;
    int max_tokens_ = 0;
    int32_t sequences_ = 0;
    int tokens_ = 0;
    llama_pos position_ = 0;
    bool use_gpu_ = false;
};

struct Models {
    std::string text;
    std::optional<std::string> media;
};

Models locate_models();

}  // namespace trowel
