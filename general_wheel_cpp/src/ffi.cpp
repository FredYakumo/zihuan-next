#include "linalg_boost/linalg_boost.hpp"

#include <algorithm>
#include <cstring>
#include <exception>
#include <string>
#include <vector>

namespace {

void write_error(const std::string& message, char* err, size_t err_len) {
    if (err == nullptr || err_len == 0) {
        return;
    }
    const auto copy_len = std::min(err_len - 1, message.size());
    std::memcpy(err, message.data(), copy_len);
    err[copy_len] = '\0';
}

template <typename Callback>
bool guard_cpp_call(Callback&& callback, char* err, size_t err_len) {
    try {
        callback();
        return true;
    } catch (const std::exception& ex) {
        write_error(ex.what(), err, err_len);
        return false;
    } catch (...) {
        write_error("unknown C++ exception", err, err_len);
        return false;
    }
}

}  // namespace

extern "C" bool gw_dot_product(
    const float* left,
    const float* right,
    size_t size,
    float* out,
    char* err,
    size_t err_len
) {
    return guard_cpp_call(
        [&]() {
            *out = wheel::linalg_boost::dot_product(left, right, size);
        },
        err,
        err_len
    );
}

extern "C" bool gw_cosine_similarity(
    const float* left,
    const float* right,
    size_t size,
    float* out,
    char* err,
    size_t err_len
) {
    return guard_cpp_call(
        [&]() {
            *out = wheel::linalg_boost::cosine_similarity(left, right, size);
        },
        err,
        err_len
    );
}

extern "C" bool gw_top_k_similar(
    const float* const* vectors,
    const float* ref_vector,
    size_t vec_size,
    size_t collection_size,
    size_t k,
    size_t* out_indices,
    float* out_scores,
    char* err,
    size_t err_len
) {
    return guard_cpp_call(
        [&]() {
            wheel::linalg_boost::top_k_similar(
                const_cast<const float**>(vectors),
                ref_vector,
                vec_size,
                collection_size,
                k,
                out_indices,
                out_scores
            );
        },
        err,
        err_len
    );
}
