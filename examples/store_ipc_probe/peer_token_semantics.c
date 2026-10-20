// Diagnostic only: distinguish current socket ownership from queued-byte origin.
// This deliberately shares an endpoint; it is not an exploit against qp.
#include <bsm/libbsm.h>
#include <errno.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>

static void require(bool ok, const char *operation) {
    if (!ok) { perror(operation); exit(1); }
}

static audit_token_t token(int fd) {
    audit_token_t value;
    socklen_t length = sizeof(value);
    require(getsockopt(fd, SOL_LOCAL, LOCAL_PEERTOKEN, &value, &length) == 0 &&
            length == sizeof(value), "LOCAL_PEERTOKEN");
    return value;
}

int main(void) {
    // Bound every failure, including a failed synchronization step.
    alarm(5);
    int stream[2], start[2], done[2], finish[2];
    require(socketpair(AF_UNIX, SOCK_STREAM, 0, stream) == 0, "socketpair");
    require(pipe(start) == 0 && pipe(done) == 0 && pipe(finish) == 0, "pipe");
    audit_token_t baseline = token(stream[0]);
    pid_t child = fork();
    require(child >= 0, "fork");
    if (child == 0) {
        close(stream[0]); close(start[1]); close(done[0]); close(finish[1]);
        char byte;
        require(read(start[0], &byte, 1) == 1, "wait start");
        require(write(stream[1], "A", 1) == 1, "child write");
        require(write(done[1], "D", 1) == 1, "notify done");
        require(read(finish[0], &byte, 1) == 1, "wait finish");
        _exit(0);
    }
    close(start[0]); close(done[1]); close(finish[0]);
    require(write(start[1], "S", 1) == 1, "start child");
    char byte;
    require(read(done[0], &byte, 1) == 1, "wait child write");
    audit_token_t before = token(stream[0]);
    // A different holder touches the sender endpoint, without writing bytes.
    // The byte written by the child remains queued on stream[0].
    char unused;
    ssize_t touch = recv(stream[1], &unused, 1, MSG_PEEK | MSG_DONTWAIT);
    require(touch == -1 && (errno == EAGAIN || errno == EWOULDBLOCK), "parent touch");
    audit_token_t after = token(stream[0]);
    require(read(stream[0], &byte, 1) == 1 && byte == 'A', "read queued child byte");
    audit_token_t after_read = token(stream[0]);
    bool writer = audit_token_to_pid(before) == child;
    bool holder = audit_token_to_pid(after) == getpid();
    bool remains = audit_token_to_pid(after_read) == getpid();
    bool baseline_restored = memcmp(&baseline, &after_read, sizeof(baseline)) == 0;
    require(write(finish[1], "F", 1) == 1, "finish child");
    int status;
    require(waitpid(child, &status, 0) == child && WIFEXITED(status) &&
            WEXITSTATUS(status) == 0, "wait child");
    printf("{\"writer_observed_before_touch\":%s,\"token_changed_to_nonwriter\":%s,"
           "\"nonwriter_still_reported_after_read\":%s,\"original_token_restored\":%s,"
           "\"queued_child_byte_received\":true,"
           "\"scope\":\"local inherited descriptor semantics, not a sandbox or qp exploit\"}\n",
           writer ? "true" : "false", holder ? "true" : "false", remains ? "true" : "false",
           baseline_restored ? "true" : "false");
    return writer && holder && remains && baseline_restored ? 0 : 2;
}
