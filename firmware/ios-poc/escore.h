/* esp-signer-core 的 C-ABI（Swift bridging header 使用）。
 * 约定：返回 >=0 成功(写入 out 的字节数)；<0 失败(|ret| 为错误信息字节数)。文本为 UTF-8。 */
#ifndef ESCORE_H
#define ESCORE_H
#include <stdint.h>
#include <stddef.h>

int escore_probe(unsigned char *out, size_t cap);

int escore_sample_unsigned(unsigned char *out, size_t cap);

int escore_import_mnemonic(const char *mnemonic, const char *password,
                           unsigned char *out, size_t cap);

int escore_generate_mnemonic(uint8_t words, unsigned char *out, size_t cap);

int escore_export_account(uint8_t coin, uint32_t account, uint8_t net,
                          const unsigned char *ks, size_t ks_len,
                          const char *password, const char *passphrase,
                          unsigned char *out, size_t cap);

int escore_wallet_info(const unsigned char *ks, size_t ks_len,
                       const char *password, const char *passphrase,
                       uint8_t net, unsigned char *out, size_t cap);

int escore_summarize(uint8_t net, uint8_t lang, const unsigned char *unsigned_data, size_t unsigned_len,
                     unsigned char *out, size_t cap);

int escore_sign(const unsigned char *unsigned_data, size_t unsigned_len,
                const unsigned char *ks, size_t ks_len,
                const char *password, const char *passphrase,
                unsigned char *out, size_t cap);

#endif
