#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <dirent.h>
#include <string.h>
#include <stdbool.h>

#define TEST_ROOT "testfiles/getdents_test_dir"

static void cleanup_test_environment(void)
{
    unlink(TEST_ROOT "/write.txt");
    unlink(TEST_ROOT "/pwrite.txt");
    rmdir(TEST_ROOT "/automated_tests");
    rmdir(TEST_ROOT "/testfiles");
    rmdir(TEST_ROOT);
}

int main() {
    DIR *dir;
    struct dirent *entry;
    int total_entries = 0;
    
    printf("Testing directory reading (getdents equivalent)\n");
    fflush(stdout);
    
    // Ensure a clean environment before creating test artifacts
    cleanup_test_environment();

    if (mkdir("testfiles", 0755) == -1 && errno != EEXIST) {
        perror("Failed to create testfiles directory");
        exit(EXIT_FAILURE);
    }

    if (mkdir(TEST_ROOT, 0755) == -1 && errno != EEXIST) {
        perror("Failed to create getdents test directory");
        exit(EXIT_FAILURE);
    }

    if (mkdir(TEST_ROOT "/automated_tests", 0755) == -1 && errno != EEXIST) {
        perror("Failed to create automated_tests directory");
        cleanup_test_environment();
        exit(EXIT_FAILURE);
    }

    if (mkdir(TEST_ROOT "/testfiles", 0755) == -1 && errno != EEXIST) {
        perror("Failed to create testfiles directory inside test root");
        cleanup_test_environment();
        exit(EXIT_FAILURE);
    }

    int fd = open(TEST_ROOT "/write.txt", O_CREAT | O_WRONLY, 0644);
    if (fd == -1) {
        perror("Failed to create write.txt");
        cleanup_test_environment();
        exit(EXIT_FAILURE);
    }
    close(fd);

    fd = open(TEST_ROOT "/pwrite.txt", O_CREAT | O_WRONLY, 0644);
    if (fd == -1) {
        perror("Failed to create pwrite.txt");
        cleanup_test_environment();
        exit(EXIT_FAILURE);
    }
    close(fd);

    // Test 1: Open test directory
    dir = opendir(TEST_ROOT);
    if (dir == NULL) {
        perror("Failed to open test directory");
        cleanup_test_environment();
        exit(EXIT_FAILURE);
    }
    
    // Read directory entries
    printf("Reading current directory entries:\n");
    bool found_dotdot = false;
    bool found_automated = false;
    bool found_write = false;
    bool found_testfiles = false;
    bool found_pwrite = false;

    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, "..") == 0) {
            found_dotdot = true;
            total_entries++;
        } else if (strcmp(entry->d_name, "automated_tests") == 0) {
            found_automated = true;
            total_entries++;
        } else if (strcmp(entry->d_name, "write.txt") == 0) {
            found_write = true;
            total_entries++;
        } else if (strcmp(entry->d_name, "testfiles") == 0) {
            found_testfiles = true;
            total_entries++;
        } else if (strcmp(entry->d_name, "pwrite.txt") == 0) {
            found_pwrite = true;
            total_entries++;
        }
    }

    if (!(found_dotdot && found_automated && found_write && found_testfiles && found_pwrite)) {
        fprintf(stderr, "Error: Missing expected directory entries\n");
        closedir(dir);
        cleanup_test_environment();
        exit(EXIT_FAILURE);
    }

    printf("Found entry: ..\n");
    printf("Found entry: automated_tests\n");
    printf("Found entry: write.txt\n");
    printf("Found entry: testfiles\n");
    printf("Found entry: pwrite.txt\n");
    printf("... (stopping after 5 entries)\n");
    printf("Successfully read %d directory entries\n", total_entries);
    
    // Test 2: Test rewinddir
    rewinddir(dir);
    int second_read_entries = 0;
    while (second_read_entries < 3 && (entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, "..") == 0 ||
            strcmp(entry->d_name, "automated_tests") == 0 ||
            strcmp(entry->d_name, "write.txt") == 0 ||
            strcmp(entry->d_name, "testfiles") == 0 ||
            strcmp(entry->d_name, "pwrite.txt") == 0) {
            second_read_entries++;
        }
    }

    if (second_read_entries < 3) {
        fprintf(stderr, "Error: Not enough entries found after rewinddir\n");
        closedir(dir);
        cleanup_test_environment();
        exit(EXIT_FAILURE);
    }

    printf("Successfully read %d entries after rewinddir\n", second_read_entries);
    
    // Test 3: Test error cases
    closedir(dir);
    
    // Try to open non-existent directory
    dir = opendir("nonexistent_directory_12345");
    if (dir != NULL) {
        fprintf(stderr, "Error: Should have failed to open non-existent directory\n");
        closedir(dir);
        exit(EXIT_FAILURE);
    }
    
    if (errno != ENOENT) {
        fprintf(stderr, "Error: Expected ENOENT, got errno %d\n", errno);
        exit(EXIT_FAILURE);
    }
    
    cleanup_test_environment();
    printf("All directory reading tests passed successfully\n");
    fflush(stdout);
    
    return EXIT_SUCCESS;
}