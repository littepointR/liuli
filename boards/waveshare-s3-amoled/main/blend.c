#include "esp_log.h"
#include "esp_timer.h"
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
extern void liuli_blend_blocks(uint16_t *aligned, unsigned blocks,
                               const uint16_t *coefficients);
static uint16_t scalar(uint16_t bg, uint16_t fg, unsigned a) {
  const unsigned inv = 15 - a;
  const unsigned r = ((bg >> 11) * inv + (fg >> 11) * a + 7) / 15;
  const unsigned g = (((bg >> 5) & 63) * inv + ((fg >> 5) & 63) * a + 7) / 15;
  const unsigned b = ((bg & 31) * inv + (fg & 31) * a + 7) / 15;
  return (r << 11) | (g << 5) | b;
}
void board_blend(uint16_t *row, size_t count, uint16_t fg, unsigned a) {
  while (count && ((uintptr_t)row & 15)) {
    *row = scalar(*row, fg, a);
    row++;
    count--;
  }
  const size_t blocks = count / 8;
  if (blocks) {
    _Alignas(16) uint16_t coeff[7][8];
    const uint16_t values[] = {31,
                               63,
                               15 - a,
                               4370,
                               (fg >> 11) * a + 7,
                               ((fg >> 5) & 63) * a + 7,
                               (fg & 31) * a + 7};
    for (unsigned j = 0; j < 7; j++)
      for (unsigned k = 0; k < 8; k++)
        coeff[j][k] = values[j];
    liuli_blend_blocks(row, blocks, &coeff[0][0]);
    row += 8 * blocks;
    count -= 8 * blocks;
  }
  while (count--) {
    *row = scalar(*row, fg, a);
    row++;
  }
}
void board_check_blend(void) {
  static _Alignas(16) uint16_t pixels[256], reference[256];
  for (unsigned a = 0; a < 16; a++) {
    for (unsigned skip = 0; skip < 8; skip++) {
      for (unsigned i = 0; i < 256; i++) {
        pixels[i] = reference[i] = (uint16_t)(i * 4591 + a);
      }
      uint16_t fg = (uint16_t)(0xac71 + 839 * a);
      unsigned end = 248 + (skip + 3) % 8;
      board_blend(pixels + skip, end - skip, fg, a);
      for (unsigned i = skip; i < end; i++)
        reference[i] = scalar(reference[i], fg, a);
      if (memcmp(reference, pixels, sizeof(pixels))) {
        ESP_LOGE("liuli", "blend mismatch alpha=%u offset=%u", a, skip);
        abort();
      }
    }
  }
  int64_t start = esp_timer_get_time();
  for (unsigned k = 0; k < 256; k++)
    for (unsigned i = 0; i < 256; i++)
      reference[i] = scalar(reference[i], (uint16_t)(k * 29), 8);
  int64_t scalar_us = esp_timer_get_time() - start;
  start = esp_timer_get_time();
  for (unsigned k = 0; k < 256; k++)
    board_blend(pixels, 256, (uint16_t)(k * 29), 8);
  int64_t simd_us = esp_timer_get_time() - start;
  if (memcmp(reference, pixels, sizeof(pixels)))
    abort();
  ESP_LOGI("liuli", "blend verified pixels=65536 scalar_us=%lld simd_us=%lld",
           (long long)scalar_us, (long long)simd_us);
}
