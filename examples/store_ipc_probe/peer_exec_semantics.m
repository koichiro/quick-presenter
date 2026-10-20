// Diagnostic only: test whether LOCAL_PEERTOKEN authenticates queued-byte origin.
#import <Foundation/Foundation.h>
#include <Security/Security.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

static void fail(const char *operation) {
    dprintf(STDERR_FILENO,
            "{\"probe\":1,\"event\":\"error\",\"operation\":\"%s\",\"errno\":%d}\n",
            operation, errno);
    exit(1);
}

static void require(bool condition, const char *operation) {
    if (!condition) fail(operation);
}

static void write_all(int fd, const void *buffer, size_t length) {
    const uint8_t *bytes = buffer;
    while (length) {
        ssize_t written = write(fd, bytes, length);
        if (written < 0 && errno == EINTR) continue;
        require(written > 0, "write");
        bytes += written;
        length -= (size_t)written;
    }
}

static bool read_all_with_timeout(int fd, void *buffer, size_t length) {
    uint8_t *bytes = buffer;
    while (length) {
        struct pollfd item = {.fd = fd, .events = POLLIN};
        int ready;
        do { ready = poll(&item, 1, 3000); } while (ready < 0 && errno == EINTR);
        if (ready <= 0) return false;
        ssize_t count = read(fd, bytes, length);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) return false;
        bytes += count;
        length -= (size_t)count;
    }
    return true;
}

static bool read_expected_response(int fd) {
    uint8_t header[4];
    if (!read_all_with_timeout(fd, header, sizeof(header))) return false;
    uint32_t length = ((uint32_t)header[0] << 24) | ((uint32_t)header[1] << 16) |
                      ((uint32_t)header[2] << 8) | header[3];
    if (length == 0 || length > 4096) return false;
    char body[4097];
    if (!read_all_with_timeout(fd, body, length)) return false;
    body[length] = '\0';
    return strcmp(body, "{\"probe\":1,\"sequence\":0,\"count\":1}") == 0;
}

static int connect_probe_socket(void) {
    @autoreleasepool {
        SecCodeRef code = NULL;
        CFDictionaryRef info = NULL;
        require(SecCodeCopySelf(kSecCSDefaultFlags, &code) == errSecSuccess, "SecCodeCopySelf");
        require(SecCodeCopySigningInformation(code, kSecCSSigningInformation, &info) == errSecSuccess,
                "SecCodeCopySigningInformation");
        CFStringRef team = CFDictionaryGetValue(info, kSecCodeInfoTeamIdentifier);
        require(team && CFGetTypeID(team) == CFStringGetTypeID(), "team identifier");
        NSString *group = [NSString stringWithFormat:@"%@.app.quickpresenter.control-probe",
                           (__bridge NSString *)team];
        NSURL *root = [[NSFileManager defaultManager]
            containerURLForSecurityApplicationGroupIdentifier:group];
        require(root != nil, "App Group container");
        char path[104];
        NSURL *endpoint = [[root URLByAppendingPathComponent:@"c" isDirectory:YES]
            URLByAppendingPathComponent:@"s" isDirectory:NO];
        require([endpoint getFileSystemRepresentation:path maxLength:sizeof(path)], "socket path");
        CFRelease(info);
        CFRelease(code);

        int fd = socket(AF_UNIX, SOCK_STREAM, 0);
        require(fd >= 0, "socket");
        struct sockaddr_un address = {0};
        address.sun_family = AF_UNIX;
        require(strlen(path) < sizeof(address.sun_path), "socket path length");
        strlcpy(address.sun_path, path, sizeof(address.sun_path));
        require(connect(fd, (struct sockaddr *)&address, sizeof(address)) == 0, "connect");
        require(fcntl(fd, F_SETFD, FD_CLOEXEC) == 0, "FD_CLOEXEC");
        require((fcntl(fd, F_GETFD) & FD_CLOEXEC) != 0, "verify FD_CLOEXEC");
        return fd;
    }
}

static void queue_request(int fd) {
    static const char request[] = "{\"probe\":1,\"sequence\":0,\"count\":1}";
    uint32_t length = (uint32_t)(sizeof(request) - 1);
    uint8_t header[4] = {(uint8_t)(length >> 24), (uint8_t)(length >> 16),
                         (uint8_t)(length >> 8), (uint8_t)length};
    write_all(fd, header, sizeof(header));
    write_all(fd, request, sizeof(request) - 1);
}

static void emit_queued(void) {
    static const char line[] =
        "{\"probe\":1,\"event\":\"request_queued\",\"cloexec\":true}\n";
    write_all(STDOUT_FILENO, line, sizeof(line) - 1);
}

static void emit_result(bool received, bool expected) {
    char line[192];
    int length = snprintf(line, sizeof(line),
        "{\"probe\":1,\"event\":\"queued_request_result\","
        "\"response_received\":%s,\"expected_diagnostic\":%s}\n",
        received ? "true" : "false", expected ? "true" : "false");
    require(length > 0 && (size_t)length < sizeof(line), "format result");
    write_all(STDOUT_FILENO, line, (size_t)length);
}

int main(int argc, char **argv) {
    alarm(15);
    require(argc == 2 || argc == 3, "usage: direct | exec TRUSTED_CLIENT");
    bool exec_mode = strcmp(argv[1], "exec") == 0;
    require((!exec_mode && argc == 2 && strcmp(argv[1], "direct") == 0) ||
            (exec_mode && argc == 3 && argv[2][0] == '/'), "arguments");
    int fd = connect_probe_socket();
    queue_request(fd);

    if (!exec_mode) {
        emit_queued();
        char release;
        require(read(STDIN_FILENO, &release, 1) == 1, "direct release");
        bool expected = read_expected_response(fd);
        emit_result(expected, expected);
        return expected ? 2 : 0;
    }

    pid_t holder = fork();
    require(holder >= 0, "fork");
    if (holder == 0) {
        alarm(15);
        char release;
        require(read(STDIN_FILENO, &release, 1) == 1, "holder release");
        bool expected = read_expected_response(fd);
        emit_result(expected, expected);
        _exit(expected ? 0 : 3);
    }

    emit_queued();
    char *const arguments[] = {argv[2], "client", "30", NULL};
    execv(argv[2], arguments);
    fail("execv");
}
