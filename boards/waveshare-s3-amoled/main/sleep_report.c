#include "esp_app_desc.h"
#include "esp_log.h"
#include "nvs.h"
#include "nvs_flash.h"
#include <stdint.h>
#include <string.h>

// A physical reset and loss of USB power must not erase the qualification
// result.
typedef struct {
  uint8_t elf_sha[32];
  uint32_t state, pixels;
  int64_t elapsed_us, transfer_us;
} wake_record_t;

enum { ARMED = 1, REDRAWN = 2 };
static const char *qa_namespace = "liuli_qa";
static const char *wake_key = "sleep_v1";

static esp_err_t store_record(const wake_record_t *record) {
  nvs_handle_t nvs;
  esp_err_t err = nvs_open(qa_namespace, NVS_READWRITE, &nvs);
  if (err != ESP_OK)
    return err;
  err = nvs_set_blob(nvs, wake_key, record, sizeof(*record));
  if (err == ESP_OK)
    err = nvs_commit(nvs);
  nvs_close(nvs);
  return err;
}

// A saved result applies only to the firmware whose ELF digest it contains.
int board_take_wake_report(void) {
  nvs_handle_t nvs;
  esp_err_t err = nvs_flash_init();
  if (err == ESP_OK)
    err = nvs_open(qa_namespace, NVS_READONLY, &nvs);
  if (err == ESP_ERR_NVS_NOT_FOUND)
    return 0;
  if (err != ESP_OK) {
    ESP_LOGE("liuli", "sleep record unavailable: %s", esp_err_to_name(err));
    return -1;
  }
  wake_record_t record;
  size_t size = sizeof(record);
  err = nvs_get_blob(nvs, wake_key, &record, &size);
  nvs_close(nvs);
  if (err == ESP_ERR_NVS_NOT_FOUND)
    return 0;
  if (err != ESP_OK || size != sizeof(record)) {
    ESP_LOGE("liuli", "invalid sleep record: %s", esp_err_to_name(err));
    return -1;
  }
  if (memcmp(record.elf_sha, esp_app_get_description()->app_elf_sha256,
             sizeof(record.elf_sha)))
    return 0;
  if (record.state == REDRAWN) {
    ESP_LOGI("liuli",
             "sample=4 index=0 elapsed_us=%lld pixels=%u transfer_us=%lld "
             "wake=timer",
             (long long)record.elapsed_us, record.pixels,
             (long long)record.transfer_us);
  } else {
    ESP_LOGW(
        "liuli",
        "sleep validation incomplete: no completed redraw for this firmware");
  }
  return 1;
}

int board_arm_sleep(void) {
  wake_record_t record = {.state = ARMED};
  memcpy(record.elf_sha, esp_app_get_description()->app_elf_sha256,
         sizeof(record.elf_sha));
  return store_record(&record);
}

int board_save_wake(unsigned pixels, int64_t elapsed, int64_t transfer) {
  wake_record_t record = {.state = REDRAWN,
                          .pixels = pixels,
                          .elapsed_us = elapsed,
                          .transfer_us = transfer};
  memcpy(record.elf_sha, esp_app_get_description()->app_elf_sha256,
         sizeof(record.elf_sha));
  return store_record(&record);
}
