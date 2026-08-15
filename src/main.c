#include <stdio.h>

struct config {
    char *target;
    int jobs;
    int threads_per_job;
    char *folders[];
};

/// Takes a config.yml/config.json that contains
/// a job list pointing to folder(s) of videos and transcodes
/// to target codecs/resolutions with progress bars
int main(int argc, char *argv[]) {

  if (argc < 2) {
    printf("==> usage: stampede <config.yaml/config.json>");
    return 1;
  }
  //

  // list all files under folders in job list
  // and open as containers for reading

  //

  return 0;
}