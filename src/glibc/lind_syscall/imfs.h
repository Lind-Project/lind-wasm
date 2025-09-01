
#include <sys/types.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/uio.h>

#include <fcntl.h>
#include <stddef.h>

#ifdef DIAG
#define LOG(...) printf(__VA_ARGS__)
#else
#define LOG(...) ((void)0)
#endif

#define MAX_NODE_NAME 65
#define MAX_NODE_SIZE 4096
#define MAX_FDS		  1024
#define MAX_NODES	  1024
#define MAX_DEPTH	  10
#define MAX_PROCS	  128

// These are stubs for the stat call, for now we return
// a constant. These can be reappropriated later.
#define GET_UID 501
#define GET_GID 20
#define GET_DEV 1

typedef struct Node Node;
typedef struct FileDesc FileDesc;
typedef struct Pipe Pipe;
typedef struct Chunk Chunk;

static int PC_CONSTS[] = {
	0,
	10,
	10,
	10,
	MAX_NODE_NAME - 1,
	MAX_DEPTH *MAX_NODE_NAME,
	10,
	10,
	10,
	10,
};

typedef enum {
	M_REG = S_IFREG,
	M_DIR = S_IFDIR,
	M_LNK = S_IFLNK,
	M_PIP,
	// Indicated free node
	M_NON = 0,
} NodeType;

#define d_children info.dir.children
#define d_count	   info.dir.count
#define l_link	   info.lnk.link
#define r_data	   info.reg.data
#define p_pipe	   info.pip.pipe
#define r_head	   info.reg.head
#define r_tail     info.reg.tail

typedef struct DirEnt {
	char name[MAX_NODE_NAME];
	struct Node *node;
} DirEnt;

typedef struct Node {
	NodeType type;
	int index;	 /* Index in the global g_nodes */
	
	size_t size; /* Size for offset related calls. */

	size_t total_size;
	
	char name[MAX_NODE_NAME]; /* File name */
	// struct Node *parent;	  /* Parent node */
	int parent_idx;
	int in_use; /* Number of FD's attached to this node */
	int doomed;
	mode_t mode;

	struct timespec atime;
	struct timespec mtime;
	struct timespec ctime;
	struct timespec btime;

	union {
		// M_REG
		struct {
			char *data; /* File contents stored as a char array */
			Chunk *head;
			Chunk *tail;
		} reg;

		// M_LNK
		struct {
			struct Node *link; /* Point to linked node. */
		} lnk;

		// M_DIR
		struct {
			struct DirEnt children[MAX_NODES]; /* Directory contents. */
			size_t count;					   /* len(children) including . and .. */
		} dir;

		// M_PIP
		struct {
			Pipe *pipe;
		} pip;
	} info;

} Node;
typedef struct FileDesc {
	int status;
	struct FileDesc *link;
	Node *node;
	int offset; /* How many bytes have been read. */
	int flags;
} FileDesc;

// This is an internal reprenstation of the DIR* struct
// the internal implementation of which changes quite often.
// We need this only to enable readdir() through opendir().
typedef struct I_DIR {
	int fd;
	Node *node;
	size_t size;
	size_t offset;
	off_t filepos;
} I_DIR;

typedef struct Pipe {
	FileDesc *readfd;
	FileDesc *writefd;
	char data[1024];
	off_t offset;
} Pipe;

typedef struct Chunk {
	char data[1024];
	size_t used;
	Chunk  *next;
} Chunk;


int imfs_open(int cage_id, const char *path, int flags, mode_t mode);
int imfs_openat(int cage_id, int dirfd, const char *path, int flags, mode_t mode);
int imfs_creat(int cage_id, const char *path, mode_t mode);
ssize_t imfs_read(int cage_id, int fd, void *buf, size_t count);
ssize_t imfs_write(int cage_id, int fd, const void *buf, size_t count);

ssize_t imfs_new_write(int cage_id, int fd, const void *buf, size_t count);
ssize_t imfs_new_read(int cage_id, int fd, const void *buf, size_t count);

int imfs_close(int cage_id, int fd);
int imfs_mkdir(int cage_id, const char *path, mode_t mode);
int imfs_mkdirat(int cage_id, int fd, const char *path, mode_t mode);
int imfs_rmdir(int cage_id, const char *path);
int imfs_remove(int cage_id, const char *path);
int imfs_link(int cage_id, const char *oldpath, const char *newpath);
int imfs_linkat(int cage_id, int olddirfd, const char *oldpath, int newdirfd, const char *newpath, int flags);
int imfs_unlink(int cage_id, const char *path);
off_t imfs_lseek(int cage_id, int fd, off_t offset, int whence);
int imfs_dup(int cage_id, int oldfd);
int imfs_dup2(int cage_id, int oldfd, int newfd);

ssize_t imfs_pwrite(int cage_id, int fd, const void *buf, size_t count, off_t offset);
ssize_t imfs_pread(int cage_id, int fd, void *buf, size_t count, off_t offset);

int imfs_lstat(int cage_id, const char *pathname, struct stat *statbuf);
int imfs_stat(int cage_id, const char *pathname, struct stat *statbuf);
int imfs_fstat(int cage_id, int fd, struct stat *statbuf);

I_DIR *imfs_opendir(int cage_id, const char *name);
struct dirent *imfs_readdir(int cage_id, I_DIR *dirstream);

ssize_t imfs_readv(int cage_id, int fd, const struct iovec *iov, int count);
ssize_t imfs_preadv(int cage_id, int fd, const struct iovec *iov, int count, off_t offset);
ssize_t imfs_writev(int cage_id, int fd, const struct iovec *iov, int count);
ssize_t imfs_pwritev(int cage_id, int fd, const struct iovec *iov, int count, off_t offset);

int imfs_symlink(int cage_id, const char *oldpath, const char *newpath);
int imfs_rename(int cage_id, const char *oldpath, const char *newpath);

int imfs_chown(int cage_id, const char *pathname, uid_t owner, gid_t group);
int imfs_chmod(int cage_id, const char *pathname, mode_t mode);
int imfs_fchmod(int cage_id, int fd, mode_t mode);

int imfs_mkfifo(int cage_id, const char *pathname, mode_t mode);
int imfs_mknod(int cage_id, const char *pathname, mode_t mode, dev_t dev);

int imfs_bind(int cage_id, int sockfd, const struct sockaddr *addr, socklen_t addrlen);

int imfs_pathconf(int cage_id, const char *pathname, int name);
int imfs_fpathconf(int cage_id, int fd, int name);

int imfs_pipe(int cage_id, int pipefd[2]);
// int pipe2(int cage_id, int pipefd[2], int flags);
int imfs_fcntl(int cage_id, int fd, int op, int arg);

void list_all_files();
void imfs_copy_fd_tables(int srcfd, int dstfd);
void imfs_init();
