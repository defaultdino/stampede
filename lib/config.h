#ifndef CONFIG_PARSER_H
#define CONFIG_PARSER_H

#pragma once

#include <yaml.h>
#include <string.h>
#include <yaml.h>

struct config {
    char *target;
    int jobs;
    int threads_per_job;
    char *folders[];
};

#endif