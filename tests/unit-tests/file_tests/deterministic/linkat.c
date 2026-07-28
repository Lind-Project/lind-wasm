#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <errno.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>

#define TEST_DIR "testfiles"
#define TEST_FILE "testfiles/linkat_test_file.txt"
#define LINK_FILE "testfiles/linkat_test_link.txt"
#define DIRFD_LINK_NAME "linkat_dirfd_link.txt"
#define DIRFD_LINK_FILE "testfiles/linkat_dirfd_link.txt"
#define SYMLINK_FILE "testfiles/linkat_test_symlink.txt"
#define FOLLOW_LINK_FILE "testfiles/linkat_follow_link.txt"

static void cleanup(void) {
    unlink(TEST_FILE);
    unlink(LINK_FILE);
    unlink(DIRFD_LINK_FILE);
    unlink(SYMLINK_FILE);
    unlink(FOLLOW_LINK_FILE);
}

int main() {
    int fd;
    struct stat stat_orig, stat_link;

    printf("Testing linkat() syscall\n");
    fflush(stdout);

    mkdir(TEST_DIR, 0755);
    cleanup();

    // Create original file
    fd = open(TEST_FILE, O_CREAT | O_WRONLY, 0644);
    if (fd == -1) {
        perror("Failed to create test file");
        exit(EXIT_FAILURE);
    }

    const char *data = "This is test data for linkat testing\n";
    if (write(fd, data, strlen(data)) == -1) {
        perror("Failed to write to test file");
        close(fd);
        exit(EXIT_FAILURE);
    }
    close(fd);

    // Test 1: Create a hard link with AT_FDCWD on both sides
    if (linkat(AT_FDCWD, TEST_FILE, AT_FDCWD, LINK_FILE, 0) == -1) {
        perror("Failed to create hard link via linkat");
        cleanup();
        exit(EXIT_FAILURE);
    }

    // Test 2: Verify both paths refer to the same inode and nlink is 2
    if (stat(TEST_FILE, &stat_orig) == -1) {
        perror("Failed to stat original file");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (stat(LINK_FILE, &stat_link) == -1) {
        perror("Failed to stat link file");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (stat_orig.st_ino != stat_link.st_ino) {
        fprintf(stderr, "Error: Original and link files have different inodes\n");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (stat_orig.st_nlink != 2) {
        fprintf(stderr, "Error: Expected 2 hard links, got %u\n",
                (unsigned)stat_orig.st_nlink);
        cleanup();
        exit(EXIT_FAILURE);
    }

    // Test 3: Verify the link reads back the same content
    char buffer[256];
    fd = open(LINK_FILE, O_RDONLY);
    if (fd == -1) {
        perror("Failed to open link file for reading");
        cleanup();
        exit(EXIT_FAILURE);
    }

    ssize_t bytes = read(fd, buffer, sizeof(buffer) - 1);
    close(fd);

    if (bytes != (ssize_t)strlen(data) || memcmp(buffer, data, bytes) != 0) {
        fprintf(stderr, "Error: Link file has different content\n");
        cleanup();
        exit(EXIT_FAILURE);
    }

    // Test 4: Create a hard link using a real directory fd on the new side
    int dirfd = open(TEST_DIR, O_RDONLY | O_DIRECTORY);
    if (dirfd == -1) {
        perror("Failed to open test directory");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (linkat(AT_FDCWD, TEST_FILE, dirfd, DIRFD_LINK_NAME, 0) == -1) {
        perror("Failed to create hard link via dirfd");
        close(dirfd);
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (stat(DIRFD_LINK_FILE, &stat_link) == -1) {
        perror("Failed to stat dirfd link file");
        close(dirfd);
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (stat_link.st_ino != stat_orig.st_ino || stat_link.st_nlink != 3) {
        fprintf(stderr, "Error: dirfd link has wrong inode or link count\n");
        close(dirfd);
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (unlinkat(dirfd, DIRFD_LINK_NAME, 0) == -1) {
        perror("Failed to unlink dirfd link");
        close(dirfd);
        cleanup();
        exit(EXIT_FAILURE);
    }
    close(dirfd);

    // Test 5: Linking onto an existing path should fail with EEXIST
    if (linkat(AT_FDCWD, TEST_FILE, AT_FDCWD, LINK_FILE, 0) != -1) {
        fprintf(stderr, "Error: Should have failed to create duplicate link\n");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (errno != EEXIST) {
        fprintf(stderr, "Error: Expected EEXIST, got errno %d\n", errno);
        cleanup();
        exit(EXIT_FAILURE);
    }

    // Test 6: Linking a non-existent oldpath should fail with ENOENT
    if (linkat(AT_FDCWD, "testfiles/linkat_nonexistent.txt", AT_FDCWD,
               "testfiles/linkat_new_link.txt", 0) != -1) {
        fprintf(stderr, "Error: Should have failed to link non-existent file\n");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (errno != ENOENT) {
        fprintf(stderr, "Error: Expected ENOENT, got errno %d\n", errno);
        cleanup();
        exit(EXIT_FAILURE);
    }

    // Test 7: AT_SYMLINK_FOLLOW links the symlink target, not the symlink
    if (symlink("linkat_test_file.txt", SYMLINK_FILE) == -1) {
        perror("Failed to create symlink");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (linkat(AT_FDCWD, SYMLINK_FILE, AT_FDCWD, FOLLOW_LINK_FILE,
               AT_SYMLINK_FOLLOW) == -1) {
        perror("Failed to create hard link with AT_SYMLINK_FOLLOW");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (lstat(FOLLOW_LINK_FILE, &stat_link) == -1) {
        perror("Failed to lstat follow link file");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (S_ISLNK(stat_link.st_mode)) {
        fprintf(stderr, "Error: AT_SYMLINK_FOLLOW produced a symlink\n");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (stat_link.st_ino != stat_orig.st_ino) {
        fprintf(stderr, "Error: AT_SYMLINK_FOLLOW link has wrong inode\n");
        cleanup();
        exit(EXIT_FAILURE);
    }

    // Test 8: Link count drops back after unlinking one name
    if (unlink(TEST_FILE) == -1) {
        perror("Failed to unlink original file");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (stat(LINK_FILE, &stat_link) == -1) {
        perror("Failed to stat link file after original deletion");
        cleanup();
        exit(EXIT_FAILURE);
    }

    if (stat_link.st_nlink != 2) {
        fprintf(stderr, "Error: Expected 2 hard links after deletion, got %u\n",
                (unsigned)stat_link.st_nlink);
        cleanup();
        exit(EXIT_FAILURE);
    }

    cleanup();

    printf("All linkat() tests passed successfully\n");
    fflush(stdout);

    return EXIT_SUCCESS;
}
