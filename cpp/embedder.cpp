#include "embedder.h"
#include "image_sizing.h"

#include "ggml.h"

#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <mutex>
#include <stdexcept>
#include <string>
#include <vector>

namespace trowel {
namespace {

class LlamaError : public std::runtime_error {
public:
    explicit LlamaError(const std::string &what) : std::runtime_error(what) {}
};

ggml_log_level g_level = GGML_LOG_LEVEL_INFO;
std::mutex g_log_mu;

void write_log(ggml_log_level level, const char *text) {
    std::lock_guard<std::mutex> lock(g_log_mu);
    if (level != GGML_LOG_LEVEL_CONT) {
        g_level = level;
    }
    if (g_level < GGML_LOG_LEVEL_WARN) {
        return;
    }
    std::fputs(text, stderr);
}

void ggml_callback(ggml_log_level level, const char *text, void *) {
    if (text) {
        write_log(level, text);
    }
}

void line(const std::string &message) {
    std::lock_guard<std::mutex> lock(g_log_mu);
    std::fprintf(stderr, "[llama.cpp worker] %s\n", message.c_str());
}

void ensure_backend() {
    static const bool inited = [] {
        llama_log_set(ggml_callback, nullptr);
        mtmd_helper_log_set(ggml_callback, nullptr);
        llama_backend_init();
        return true;
    }();
    (void)inited;
}

std::string env_or(const char *name) {
    const char *v = std::getenv(name);
    return v ? std::string(v) : std::string();
}

std::string home_dir() {
    auto h = env_or("HOME");
    return h.empty() ? "/" : h;
}

bool file_exists(const std::string &path) {
    if (path.empty()) {
        return false;
    }
    if (FILE *f = std::fopen(path.c_str(), "rb")) {
        std::fclose(f);
        return true;
    }
    return false;
}

std::vector<std::string> model_folders() {
    for (const char *key : {"TROWEL_MODELS", "DIGUP_MODELS"}) {
        auto chosen = env_or(key);
        if (!chosen.empty()) {
            return {chosen};
        }
    }
    std::string xdg = env_or("XDG_DATA_HOME");
    std::string installed =
        (xdg.empty() ? home_dir() + "/.local/share" : xdg) + "/trowel/models";
    std::string hub = env_or("HF_HUB_CACHE");
    if (hub.empty()) {
        auto hf = env_or("HF_HOME");
        hub = hf.empty() ? home_dir() + "/.cache/huggingface/hub" : hf + "/hub";
    }
    std::string snap = hub + "/models--ggml-org--embeddinggemma-2-GGUF/snapshots/"
                       "bfcd298762cc34d0357ece5ebdd31791a3a374d8";
    return {installed, snap};
}

}  // namespace

Models locate_models() {
    const char *text_name = "embeddinggemma-2-Q8_0.gguf";
    const char *media_name = "mmproj-embeddinggemma-2-Q8_0.gguf";
    std::string tried;
    for (const auto &folder : model_folders()) {
        if (!tried.empty()) {
            tried += ", ";
        }
        tried += folder;
        const std::string text = folder + "/" + text_name;
        if (!file_exists(text)) {
            continue;
        }
        Models m;
        m.text = text;
        const std::string media = folder + "/" + media_name;
        if (file_exists(media)) {
            m.media = media;
        }
        return m;
    }
    throw LlamaError(std::string("no ") + text_name + " in " + tried);
}

Embedder::Embedder(const std::string &text_path, const std::optional<std::string> &media_path,
                   bool text_only, bool use_gpu)
    : use_gpu_(use_gpu) {
    const auto started = std::chrono::steady_clock::now();
    ensure_backend();
    if (!text_only && !media_path) {
        throw LlamaError("mmproj-embeddinggemma-2-Q8_0.gguf (images and audio) isn't downloaded yet");
    }

    llama_model_params model_params = llama_model_default_params();
    model_params.n_gpu_layers = use_gpu ? 999 : 0;
    model_ = llama_model_load_from_file(text_path.c_str(), model_params);
    if (!model_) {
        throw LlamaError("can't load " + text_path);
    }

    max_tokens_ = text_only ? 512 : 2048;
    llama_context_params params = llama_context_default_params();
    params.n_ctx = static_cast<uint32_t>(max_tokens_);
    params.n_batch = static_cast<uint32_t>(max_tokens_);
    params.n_ubatch = static_cast<uint32_t>(max_tokens_);
    params.n_seq_max = static_cast<uint32_t>(kMaxSequences);
    params.kv_unified = true;
    params.embeddings = true;
    params.pooling_type = LLAMA_POOLING_TYPE_MEAN;
    context_ = llama_init_from_model(model_, params);
    if (!context_) {
        llama_model_free(model_);
        model_ = nullptr;
        throw LlamaError("can't create a llama.cpp context");
    }

    if (media_path && !text_only) {
        mtmd_context_params media_params = mtmd_context_params_default();
        media_params.use_gpu = use_gpu;
        media_params.print_timings = false;
        media_params.warmup = true;
        media_params.image_min_tokens = 1;
        media_params.image_max_tokens = 1120;
        media_ = mtmd_init_from_file(media_path->c_str(), model_, media_params);
        if (!media_) {
            llama_free(context_);
            llama_model_free(model_);
            context_ = nullptr;
            model_ = nullptr;
            throw LlamaError("can't load " + *media_path);
        }
    }

    batch_ = llama_batch_ext_init(context_);
    vocab_ = llama_model_get_vocab(model_);
    embedding_size_ = llama_model_n_embd_out(model_);
    row_size_ = llama_model_n_embd_inp(model_);
    info_.dim = embedding_size_;
    info_.text_only = text_only;

    try {
        for (int i = 0; i < 2; ++i) {
            embed_texts({"task: search result | query: warm up"});
        }
    } catch (...) {
        close();
        throw;
    }

    const auto elapsed = std::chrono::steady_clock::now() - started;
    info_.load_seconds = std::round(std::chrono::duration<double>(elapsed).count() * 100.0) / 100.0;
    line("ready in " + std::to_string(info_.load_seconds) + " s (" + (text_only ? "text-only" : "full") +
         ", llama.cpp " + kBuildTag + ", vectors " + std::to_string(kVectorVersion) + ")");
}

Embedder::~Embedder() { close(); }

void Embedder::close() {
    if (batch_) {
        llama_batch_ext_free(batch_);
        batch_ = nullptr;
    }
    if (media_) {
        mtmd_free(media_);
        media_ = nullptr;
    }
    if (context_) {
        llama_free(context_);
        context_ = nullptr;
    }
    if (model_) {
        llama_model_free(model_);
        model_ = nullptr;
    }
}

std::vector<llama_token> Embedder::tokenize(const std::string &text) const {
    // `<bos> text <eos>`, cut to maxTokens keeping both special tokens.
    // upstream/Sources/LlamaRuntime/LlamaEmbedder.swift:234
    std::vector<llama_token> tokens(text.size() + 8);
    int32_t count = llama_tokenize(vocab_, text.c_str(), static_cast<int32_t>(text.size()), tokens.data(),
                                   static_cast<int32_t>(tokens.size()), true, true);
    if (count < 0) {
        tokens.assign(static_cast<std::size_t>(-count), 0);
        count = llama_tokenize(vocab_, text.c_str(), static_cast<int32_t>(text.size()), tokens.data(),
                               static_cast<int32_t>(tokens.size()), true, true);
    }
    if (count < 0) {
        tokens.clear();
        return tokens;
    }
    tokens.resize(static_cast<std::size_t>(count));
    if (static_cast<int>(tokens.size()) > max_tokens_) {
        tokens.resize(static_cast<std::size_t>(max_tokens_ - 1));
        tokens.push_back(llama_vocab_eos(vocab_));
    }
    return tokens;
}

void Embedder::begin() {
    sequences_ += 1;
    position_ = 0;
}

void Embedder::reset() {
    llama_batch_ext_clear(batch_);
    sequences_ = 0;
    tokens_ = 0;
}

void Embedder::place(int32_t index) {
    if (index < 0) {
        throw LlamaError("batch full (" + std::to_string(index) + ")");
    }
    llama_pos pos[4] = {position_, 0, 0, 0};
    if (!llama_batch_ext_set_pos(batch_, index, pos) || !llama_batch_ext_set_output_embd(batch_, index, true)) {
        throw LlamaError("can't place token " + std::to_string(index));
    }
    position_ += 1;
    tokens_ += 1;
}

void Embedder::add_token(llama_token token) { place(llama_batch_ext_add_token(batch_, sequences_ - 1, token)); }

void Embedder::add_rows(float *rows, int count) {
    for (int row = 0; row < count; ++row) {
        llama_embd embd{rows + row * row_size_, 1, static_cast<size_t>(row_size_)};
        place(llama_batch_ext_add_embd(batch_, sequences_ - 1, embd));
    }
}

std::vector<std::vector<float>> Embedder::run() {
    struct Reset {
        Embedder *self;
        ~Reset() { self->reset(); }
    } guard{this};

    const int32_t status = llama_process(context_, LLAMA_PROCESS_TYPE_ENCODE, batch_);
    if (status != 0) {
        throw LlamaError("llama_process failed (" + std::to_string(status) + ")");
    }
    std::vector<std::vector<float>> out;
    out.reserve(static_cast<std::size_t>(sequences_));
    for (int32_t sequence = 0; sequence < sequences_; ++sequence) {
        float *pooled = llama_get_embeddings_seq(context_, sequence);
        if (!pooled) {
            throw LlamaError("no pooled embedding for input " + std::to_string(sequence));
        }
        double sum = 0;
        for (int i = 0; i < embedding_size_; ++i) {
            sum += static_cast<double>(pooled[i]) * static_cast<double>(pooled[i]);
        }
        const float scale = sum > 0 ? static_cast<float>(1.0 / std::sqrt(sum)) : 0;
        std::vector<float> vec(static_cast<std::size_t>(embedding_size_));
        for (int i = 0; i < embedding_size_; ++i) {
            vec[static_cast<std::size_t>(i)] = pooled[i] * scale;
        }
        out.push_back(std::move(vec));
    }
    return out;
}

std::vector<std::vector<float>> Embedder::embed_texts(const std::vector<std::string> &texts) {
    reset();
    std::vector<std::vector<float>> vectors;
    for (const auto &text : texts) {
        auto toks = tokenize(text);
        if (sequences_ == kMaxSequences || tokens_ + static_cast<int>(toks.size()) > max_tokens_) {
            auto more = run();
            vectors.insert(vectors.end(), more.begin(), more.end());
        }
        begin();
        for (llama_token tok : toks) {
            add_token(tok);
        }
    }
    if (sequences_ > 0) {
        auto more = run();
        vectors.insert(vectors.end(), more.begin(), more.end());
    }
    return vectors;
}

Embedding make_embedding(const std::function<std::vector<float>()> &make) {
    try {
        return Embedding{make(), std::nullopt};
    } catch (const std::exception &e) {
        std::string msg = e.what();
        if (msg.size() > 300) {
            msg.resize(300);
        }
        return Embedding{std::nullopt, msg};
    }
}

std::vector<Embedding> Embedder::embed_images(const std::vector<std::string> &paths, int budget) {
    if (!media_) {
        throw LlamaError("this embedder was started text-only");
    }
    static const int budgets[] = {70, 140, 280, 560, 1120};
    bool ok = false;
    for (int b : budgets) {
        if (b == budget) {
            ok = true;
            break;
        }
    }
    if (!ok) {
        throw LlamaError("budget must be one of 70, 140, 280, 560, 1120");
    }
    std::vector<Embedding> out;
    out.reserve(paths.size());
    for (const auto &path : paths) {
        out.push_back(make_embedding([&] { return embed_media(path, true, budget); }));
    }
    return out;
}

std::vector<Embedding> Embedder::embed_audio(const std::vector<std::string> &paths) {
    if (!media_) {
        throw LlamaError("this embedder was started text-only");
    }
    std::vector<Embedding> out;
    out.reserve(paths.size());
    for (const auto &path : paths) {
        out.push_back(make_embedding([&] { return embed_media(path, false, 0); }));
    }
    return out;
}

std::vector<float> Embedder::embed_media(const std::string &path, bool image, int budget) {
    mtmd_bitmap *bitmap = load(path, image, budget);
    struct FreeBitmap {
        mtmd_bitmap *b;
        ~FreeBitmap() {
            if (b) {
                mtmd_bitmap_free(b);
            }
        }
    } free_bitmap{bitmap};

    mtmd_input_chunks *chunks = mtmd_input_chunks_init();
    struct FreeChunks {
        mtmd_input_chunks *c;
        ~FreeChunks() { mtmd_input_chunks_free(c); }
    } free_chunks{chunks};

    const char *marker = mtmd_default_marker();
    mtmd_input_text text{marker, std::strlen(marker), true, true};
    const mtmd_bitmap *bitmaps[1] = {bitmap};
    const int32_t status = mtmd_tokenize(media_, chunks, &text, bitmaps, 1);
    if (status != 0) {
        throw LlamaError("can't tokenize " + path + " (" + std::to_string(status) + ")");
    }
    reset();
    begin();
    const size_t n = mtmd_input_chunks_size(chunks);
    for (size_t index = 0; index < n; ++index) {
        const mtmd_input_chunk *chunk = mtmd_input_chunks_get(chunks, index);
        if (mtmd_input_chunk_get_type(chunk) == MTMD_INPUT_CHUNK_TYPE_TEXT) {
            size_t count = 0;
            const llama_token *tokens = mtmd_input_chunk_get_tokens_text(chunk, &count);
            if (!tokens) {
                continue;
            }
            for (size_t i = 0; i < count; ++i) {
                add_token(tokens[i]);
            }
        } else {
            if (mtmd_encode_chunk(media_, chunk) != 0) {
                throw LlamaError("the encoder failed on " + path);
            }
            float *rows = mtmd_get_output_embd(media_);
            if (!rows) {
                throw LlamaError("the encoder failed on " + path);
            }
            add_rows(rows, static_cast<int>(mtmd_input_chunk_get_n_tokens(chunk)));
        }
    }
    return run()[0];
}

mtmd_bitmap *Embedder::load(const std::string &path, bool image, int budget) {
    mtmd_helper_bitmap_wrapper wrapper =
        mtmd_helper_bitmap_init_from_file(media_, path.c_str(), false, mtmd_helper_init_opt_default());
    if (wrapper.video_ctx) {
        if (wrapper.bitmap) {
            mtmd_bitmap_free(wrapper.bitmap);
        }
        mtmd_helper_video_free(wrapper.video_ctx);
        throw LlamaError("video goes in as frames and sound, not as a file: " + path);
    }
    mtmd_bitmap *bitmap = wrapper.bitmap;
    if (!bitmap) {
        throw LlamaError("can't read " + path);
    }
    if (mtmd_bitmap_is_audio(bitmap) == image) {
        mtmd_bitmap_free(bitmap);
        throw LlamaError(std::string("not ") + (image ? "an image" : "audio") + ": " + path);
    }
    if (image) {
        const int width = static_cast<int>(mtmd_bitmap_get_nx(bitmap));
        const int height = static_cast<int>(mtmd_bitmap_get_ny(bitmap));
        auto target = target_size(height, width, budget);
        if (!target) {
            mtmd_bitmap_free(bitmap);
            throw LlamaError("image too thin to embed: " + path);
        }
        if (target->height == height && target->width == width) {
            return bitmap;
        }
        auto pixels = resize_bicubic(mtmd_bitmap_get_data(bitmap), width, height, target->width, target->height);
        mtmd_bitmap_free(bitmap);
        mtmd_bitmap *resized =
            mtmd_bitmap_init(static_cast<uint32_t>(target->width), static_cast<uint32_t>(target->height),
                             pixels.data());
        if (!resized) {
            throw LlamaError("can't resize " + path);
        }
        return resized;
    }
    const size_t samples = mtmd_bitmap_get_n_bytes(bitmap) / sizeof(float);
    const size_t limit = static_cast<size_t>(30 * mtmd_get_audio_sample_rate(media_));
    if (samples <= limit) {
        return bitmap;
    }
    const auto *data = reinterpret_cast<const float *>(mtmd_bitmap_get_data(bitmap));
    mtmd_bitmap *prefix = mtmd_bitmap_init_from_audio(limit, data);
    mtmd_bitmap_free(bitmap);
    if (!prefix) {
        throw LlamaError("can't cut " + path + " at 30 s");
    }
    return prefix;
}

}  // namespace trowel
