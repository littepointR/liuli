#include "driver/gpio.h"
#include "driver/i2c_master.h"
#include "driver/spi_master.h"
#include "esp_attr.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_partition.h"
#include "esp_sleep.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include <stdint.h>
#include <stdlib.h>

static spi_device_handle_t panel;
static i2c_master_dev_handle_t touch;
static int64_t transfer_us;
static const esp_partition_t *font_partition;
void *board_alloc(size_t size, size_t alignment) {
  return heap_caps_aligned_alloc(alignment, size,
                                 MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
}
void board_free(void *ptr) { heap_caps_free(ptr); }
int board_read_font(size_t offset, uint8_t *data, size_t len) {
  return esp_partition_read(font_partition, offset, data, len);
}
void board_font_metrics(uint64_t reads, uint64_t bytes, uint64_t hits,
                        size_t size) {
  ESP_LOGI("liuli", "font_probe reads=%llu bytes=%llu hits=%llu psram_cache=%u",
           (unsigned long long)reads, (unsigned long long)bytes,
           (unsigned long long)hits, (unsigned)size);
}
static unsigned font_requests, font_repeated, font_reads, font_bytes;
void board_font_frame(unsigned requests, unsigned repeated, unsigned reads,
                      unsigned bytes) {
  font_requests = requests;
  font_repeated = repeated;
  font_reads = reads;
  font_bytes = bytes;
}
void board_begin_sample(void) { transfer_us = 0; }
// USB Serial/JTAG may disconnect in light sleep. Retain the result across one
// software reset.
static RTC_NOINIT_ATTR struct {
  uint32_t magic, pixels;
  int64_t elapsed, transfer;
} wake_report;
int board_take_wake_report(void) {
  if (esp_reset_reason() != ESP_RST_SW || wake_report.magic != 0x4c575231) {
    wake_report.magic = 0;
    return 0;
  }
  wake_report.magic = 0;
  ESP_LOGI(
      "liuli",
      "sample=4 index=0 elapsed_us=%lld pixels=%u transfer_us=%lld wake=timer",
      (long long)wake_report.elapsed, wake_report.pixels,
      (long long)wake_report.transfer);
  return 1;
}
void board_sample(unsigned kind, unsigned n, int64_t elapsed, unsigned pixels) {
  if (kind == 4) {
    wake_report.elapsed = elapsed;
    wake_report.transfer = transfer_us;
    wake_report.pixels = pixels;
    wake_report.magic = 0x4c575231;
    esp_restart();
  }
  ESP_LOGI("liuli",
           "sample=%u index=%u elapsed_us=%lld pixels=%u transfer_us=%lld "
           "glyph_requests=%u cross_band=%u font_reads=%u font_bytes=%u "
           "stack_free_min=%u",
           kind, n, (long long)elapsed, pixels, (long long)transfer_us,
           font_requests, font_repeated, font_reads, font_bytes,
           (unsigned)uxTaskGetStackHighWaterMark(NULL));
  transfer_us = 0;
}

static esp_err_t send(uint8_t op, uint8_t reg, const void *bytes, size_t n) {
  spi_transaction_t tr = {.cmd = op,
                          .addr = (uint32_t)reg << 8,
                          .length = n * 8,
                          .tx_buffer = bytes,
                          .flags = op == 0x32 ? SPI_TRANS_MODE_QIO : 0};
  return spi_device_polling_transmit(panel, &tr);
}
static void setup(void) {
  const spi_bus_config_t bus = {.sclk_io_num = 47,
                                .data0_io_num = 18,
                                .data1_io_num = 7,
                                .data2_io_num = 48,
                                .data3_io_num = 5,
                                .data4_io_num = -1,
                                .data5_io_num = -1,
                                .data6_io_num = -1,
                                .data7_io_num = -1,
                                .max_transfer_sz = 240 * 8 * 2};
  ESP_ERROR_CHECK(spi_bus_initialize(SPI2_HOST, &bus, SPI_DMA_CH_AUTO));
  const spi_device_interface_config_t dev = {.command_bits = 8,
                                             .address_bits = 24,
                                             .clock_speed_hz = 40 * 1000 * 1000,
                                             .spics_io_num = 6,
                                             .queue_size = 1,
                                             .flags = SPI_DEVICE_HALFDUPLEX};
  ESP_ERROR_CHECK(spi_bus_add_device(SPI2_HOST, &dev, &panel));
  ESP_ERROR_CHECK(gpio_set_direction(17, GPIO_MODE_OUTPUT));
  gpio_set_level(17, 0);
  vTaskDelay(pdMS_TO_TICKS(20));
  gpio_set_level(17, 1);
  vTaskDelay(pdMS_TO_TICKS(120));
  ESP_ERROR_CHECK(send(2, 0x11, NULL, 0));
  vTaskDelay(pdMS_TO_TICKS(120));
  ESP_ERROR_CHECK(send(2, 0x36, (uint8_t[]){0x00}, 1));
  ESP_ERROR_CHECK(send(2, 0x3A, (uint8_t[]){0x55}, 1));
  ESP_ERROR_CHECK(send(2, 0x29, NULL, 0));
  vTaskDelay(pdMS_TO_TICKS(20));
  ESP_ERROR_CHECK(send(2, 0x51, (uint8_t[]){0xA0}, 1));
  const i2c_master_bus_config_t conf = {.i2c_port = I2C_NUM_0,
                                        .sda_io_num = 40,
                                        .scl_io_num = 39,
                                        .clk_source = I2C_CLK_SRC_DEFAULT,
                                        .glitch_ignore_cnt = 7,
                                        .flags.enable_internal_pullup = 1};
  i2c_master_bus_handle_t i2c;
  ESP_ERROR_CHECK(i2c_new_master_bus(&conf, &i2c));
  const i2c_device_config_t ft = {.dev_addr_length = I2C_ADDR_BIT_LEN_7,
                                  .device_address = 0x38,
                                  .scl_speed_hz = 300000};
  ESP_ERROR_CHECK(i2c_master_bus_add_device(i2c, &ft, &touch));
  ESP_ERROR_CHECK(i2c_master_transmit(touch, (uint8_t[]){0, 0}, 2, 100));
}
int board_pixels(int x, int y, unsigned w, unsigned h, const uint16_t *pixels) {
  if (x < 0 || y < 0 || w == 0 || h == 0 || x + w > 240 || y + h > 536)
    return ESP_ERR_INVALID_ARG;
  const int64_t start = esp_timer_get_time();
  unsigned x1 = x + w - 1, y1 = y + h - 1;
  esp_err_t e = send(2, 0x2A, (uint8_t[]){x >> 8, x, x1 >> 8, x1}, 4);
  if (e == ESP_OK)
    e = send(2, 0x2B, (uint8_t[]){y >> 8, y, y1 >> 8, y1}, 4);
  if (e == ESP_OK)
    e = send(0x32, 0x2C, pixels, w * h * 2);
  transfer_us += esp_timer_get_time() - start;
  return e;
}
typedef struct {
  int32_t x, y;
  uint8_t id;
} board_contact_t;
int board_touch(board_contact_t *points) {
  uint8_t reg = 2, data[11];
  if (i2c_master_transmit_receive(touch, &reg, 1, data, sizeof(data), 50) !=
      ESP_OK)
    return -1;
  const unsigned count = data[0] & 15;
  if (count > 2)
    return -1;
  static unsigned peak = 0;
  if (count > peak) {
    peak = count;
    ESP_LOGI("liuli", "touch_contacts_peak=%u", peak);
  }
  for (unsigned i = 0; i < count; i++) {
    const uint8_t *pt = &data[1 + 6 * i];
    points[i] = (board_contact_t){.x = ((pt[0] & 15) << 8) | pt[1],
                                  .y = ((pt[2] & 15) << 8) | pt[3],
                                  .id = pt[2] >> 4};
    if (points[i].x >= 240 || points[i].y >= 536 ||
        (i && points[0].id == points[i].id))
      return -1;
  }
  return count;
}
int board_sleep_cycle(void) {
  esp_err_t err = send(2, 0x28, NULL, 0);
  if (err == ESP_OK)
    err = send(2, 0x10, NULL, 0);
  if (err == ESP_OK)
    err = esp_sleep_enable_timer_wakeup(100000);
  if (err == ESP_OK)
    err = esp_light_sleep_start();
  if (err == ESP_OK && esp_sleep_get_wakeup_cause() != ESP_SLEEP_WAKEUP_TIMER)
    err = ESP_FAIL;
  esp_sleep_disable_wakeup_source(ESP_SLEEP_WAKEUP_TIMER);
  if (err == ESP_OK)
    err = send(2, 0x11, NULL, 0);
  vTaskDelay(pdMS_TO_TICKS(120));
  if (err == ESP_OK)
    err = send(2, 0x29, NULL, 0);
  return err;
}
int64_t board_now_us(void) { return esp_timer_get_time(); }
void board_wait(void) { vTaskDelay(pdMS_TO_TICKS(10)); }
void board_frame(unsigned pixels, int64_t elapsed_us) {
  ESP_LOGI("liuli",
           "timestamp_us=%lld pixels=%u frame_us=%lld transfer_us=%lld "
           "free_internal=%u free_psram=%u",
           (long long)board_now_us(), pixels, (long long)elapsed_us,
           (long long)transfer_us,
           (unsigned)heap_caps_get_free_size(MALLOC_CAP_INTERNAL),
           (unsigned)heap_caps_get_free_size(MALLOC_CAP_SPIRAM));
  transfer_us = 0;
}
extern int liuli_run(const uint8_t *font, size_t len);
extern void board_check_blend(void);
extern int liuli_benchmark(void);
extern unsigned liuli_static_bytes(void);
static void ui_main(void *arg) {
  (void)arg;
  if (liuli_benchmark())
    board_check_blend();
  setup();
  const esp_partition_t *font =
      esp_partition_find_first(ESP_PARTITION_TYPE_DATA, 0x40, "fonts");
  font_partition = font;
  if (!font) {
    ESP_LOGE("liuli", "font partition missing");
    vTaskDelete(NULL);
    return;
  }
  const void *ptr;
  esp_partition_mmap_handle_t map;
  ESP_ERROR_CHECK(esp_partition_mmap(font, 0, font->size,
                                     ESP_PARTITION_MMAP_DATA, &ptr, &map));
  int result = liuli_run(ptr, font->size);
  esp_partition_munmap(map);
  ESP_LOGE("liuli", "UI stopped (%d)", result);
  vTaskDelete(NULL);
}
extern unsigned liuli_task_stack(void);
void app_main(void) {
  const unsigned stack = liuli_task_stack();
  ESP_LOGI("liuli", "ui_task_stack_reserved=%u ui_static_with_bands=%u", stack,
           liuli_static_bytes());
  if (xTaskCreate(ui_main, "liuli", stack, NULL, 5, NULL) != pdPASS) {
    ESP_LOGE("liuli", "UI task allocation failed");
  }
}
