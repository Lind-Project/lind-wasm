#include <sys/fcntl.h>
#include <sys/stat.h>
#include <sys/uio.h>

#include <dirent.h>
#include <errno.h>
#include <time.h>
//#ifdef DIAG
#include <stdio.h>
//#endif
#include <sys/mman.h>

#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include "imfs.h"

struct IMFState {
	Node nodes[1024];
	int next_node;
	int free_list[MAX_NODES];
	int free_list_size;
};

static struct IMFState g_state;

#define g_next_node		 g_state.next_node
#define g_nodes			 g_state.nodes
#define g_free_list		 g_state.free_list
#define g_free_list_size 	 g_state.free_list_size

// static Node *g_nodes;

// Each Process (Cage) has it's own FD Table, all of which are initiated
// in memory when imfs_init() is called. Node are allocated using the use of
// g_next_node and g_free_list, as described below.
//
// This tracks "Holes" in the g_nodes table, caused by nodes that were deleted.
// When creating a new node, we check which index this free list points to and creates
// the node there. In case there are no free nodes in this list, we use the global
// g_next_node index.

// static int g_next_node = 0;
// static int g_free_list[MAX_NODES];
// static int g_free_list_size = -1;

static FileDesc g_fdtable[MAX_PROCS][MAX_FDS];

// We use the same logic for fd allocations.
static int g_next_fd[MAX_PROCS];
static int g_fd_free_list[MAX_PROCS][MAX_FDS];
static int g_fd_free_list_size[MAX_PROCS];

static Node *g_root_node = NULL;

//
// String Utils
//

static size_t
str_len(const char *name)
{
	int i = 0;
	while (name[i] != '\0') {
		i++;
	}
	return i;
}

static char *
str_rchr(const char *s, const char c)
{
	char *last = 0;

	while (*s != '\0') {
		if (*s == (char)c) {
			last = (char *)s;
		}
		s++;
	}

	if (c == '\0') {
		return (char *)s;
	}

	return last;
}

static void
split_path(const char *path, int *count, char namecomp[MAX_DEPTH][MAX_NODE_NAME])
{
	*count = 0;

	int i = 0;
	if (path[i] == '/')
		i++;

	int current_len = 0;
	while (path[i] != '\0') {
		if (path[i] == '/') {
			namecomp[*count][current_len] = '\0';
			(*count)++;
			current_len = 0;
		} else {
			namecomp[*count][current_len++] = path[i];
		}

		i++;
	}
	namecomp[*count][current_len] = '\0';
	(*count)++;
}

static int
str_compare(const char *a, const char *b)
{
	int a_len = 0;
	while (a[a_len] != '\0')
		a_len++;
	int b_len = 0;
	while (b[b_len] != '\0')
		b_len++;

	if (a_len != b_len)
		return 0;
	int i = 0, j = 0;
	while (a[i] != '\0' && b[j] != '\0') {
		if (a[i] != b[j])
			return 0;
		i++;
		j++;
	}
	return 1;
}

static void
str_ncopy(char *dst, const char *src, int n)
{
	size_t i;
	for (i = 0; i < n && src[i] != '\0'; i++) {
		dst[i] = src[i];
	}
}

static void
mem_cpy(void *dst, const void *src, size_t n)
{
	size_t i;
	unsigned char *d = dst;
	const unsigned char *s = src;

	for (i = 0; i < n; i++) {
		d[i] = s[i];
	}
}

//
//  IMFS Utils
//

void
imfs_copy_fd_tables(int srcfd, int dstfd)
{
	for (int i = 0; i < MAX_FDS; i++) {
		g_fdtable[dstfd][i] = g_fdtable[srcfd][i];
	}
}

static Node *
imfs_create_node(const char *name, NodeType type, mode_t mode)
{
	if (g_free_list_size == -1 && g_next_node >= MAX_NODES) {
		errno = ENOMEM;
		return NULL;
	}

	int node_index;
	if (g_free_list_size == -1)
		node_index = g_next_node++;
	else
		node_index = g_free_list[g_free_list_size--];

	if (g_nodes[node_index].type != M_NON) {
		errno = ENOMEM;
		return NULL;
	}

	g_nodes[node_index].in_use = 0;
	g_nodes[node_index].type = type;
	g_nodes[node_index].size = 0;
	g_nodes[node_index].d_count = 0;
	// g_nodes[node_index].d_children = NULL;
	g_nodes[node_index].r_data = NULL;
	g_nodes[node_index].parent_idx = -1;
	g_nodes[node_index].mode = g_nodes[node_index].type | (mode & 0777);

	clock_gettime(CLOCK_REALTIME, &g_nodes[node_index].atime);
	clock_gettime(CLOCK_REALTIME, &g_nodes[node_index].btime);
	clock_gettime(CLOCK_REALTIME, &g_nodes[node_index].ctime);
	clock_gettime(CLOCK_REALTIME, &g_nodes[node_index].mtime);

	str_ncopy(g_nodes[node_index].name, name, MAX_NODE_NAME);
	g_nodes[node_index].name[MAX_NODE_NAME - 1] = '\0';
	
	fprintf(stderr, "\n[imfs] created node: %s [%d]\n", g_nodes[node_index].name, node_index); 

	return &g_nodes[node_index];
}

static int
imfs_allocate_fd(int cage_id, Node *node, int flags)
{
	if (!node)
		return -1;

	int i;
	if (g_fd_free_list_size[cage_id] > -1) {
		i = g_fd_free_list[cage_id][g_fd_free_list_size[cage_id]--];
	} else {
		i = g_next_fd[cage_id]++;
	}

	if (i == MAX_FDS) {
		errno = EMFILE;
		return -1;
	}

	g_fdtable[cage_id][i] = (FileDesc) {
		.node = node,
		.offset = 0,
		.link = NULL,
		.status = 1,
		.flags = flags,
	};

	node->in_use++;

	clock_gettime(CLOCK_REALTIME, &node->atime);

	return i;
}

static int
imfs_dup_fd(int cage_id, int oldfd, int newfd)
{
	if (newfd == oldfd)
		return newfd;

	int i;
	if (newfd != -1) {
		i = newfd;
		goto allocate;
	}

	if (g_fd_free_list_size[cage_id] > -1) {
		i = g_fd_free_list[cage_id][g_fd_free_list_size[cage_id]--];
	} else {
		i = g_next_fd[cage_id]++;
	}

	if (i == MAX_FDS) {
		errno = EMFILE;
		return -1;
	}

allocate:

	if (g_fdtable[cage_id][i].node || g_fdtable[cage_id][i].link)
		imfs_close(cage_id, i);

	g_fdtable[cage_id][i] = (FileDesc) {
		.link = &g_fdtable[cage_id][oldfd],
		.node = NULL,
		.offset = 0,
	};

	return i;
}

static FileDesc *
get_filedesc(int cage_id, int fd)
{
	if (g_fdtable[cage_id][fd].link)
		return g_fdtable[cage_id][fd].link;

	return &g_fdtable[cage_id][fd];
}

static Node *
imfs_find_node_namecomp(int cage_id, int dirfd, const char namecomp[MAX_DEPTH][MAX_NODE_NAME], int count)
{
	FileDesc *fd = get_filedesc(cage_id, dirfd);
	if (count == 0)
		return g_root_node;

	Node *current;
	if (dirfd == AT_FDCWD)
		current = g_root_node;
	else
		current = fd->node;

	for (int i = 0; i < count && current; i++) {
		Node *found = NULL;
		for (size_t j = 0; j < current->d_count; j++) {
			if (str_compare(namecomp[i], current->d_children[j].name) == 1) {
				switch (current->d_children[j].node->type) {
				case M_LNK:
					found = current->d_children[j].node->l_link;
					break;
				case M_DIR:
				case M_REG:
					found = current->d_children[j].node;
					break;
				default:
					found = NULL;
				}
				break;
			}
		}

		if (!found) {
			return NULL;
		}

		current = found;
	}

	return current;
}

static Node *
imfs_find_node(int cage_id, int dirfd, const char *path)
{
	if (!path || !g_root_node)
		return NULL;

	if (path[0] == '/' && path[1] == '\0')
		return g_root_node;

	int count;
	char namecomps[MAX_DEPTH][MAX_NODE_NAME];

	split_path(path, &count, namecomps);

	return imfs_find_node_namecomp(cage_id, dirfd, namecomps, count);
}

static int
add_child(Node *parent, Node *node)
{
	if (!parent || !node || parent->type != M_DIR)
		return -1;

	size_t new_count = parent->d_count + 1;
	// DirEnt *new_children = realloc(parent->d_children, new_count * sizeof(DirEnt));

	parent->d_children[parent->d_count].node = node;

	// parent->d_children = new_children;

	str_ncopy(parent->d_children[parent->d_count].name, node->name, MAX_NODE_NAME);
	parent->d_count = new_count;
	node->parent_idx = parent->index;

	return 0;
}

static int
imfs_remove_file(Node *node)
{
	g_nodes[node->parent_idx].d_count--;

	node->doomed = 1;

	if (!node->in_use) {
		g_free_list[++g_free_list_size] = node->index;
		node->type = M_NON;
	}

	return 0;
}

static int
imfs_remove_dir(Node *node)
{
	if (node == g_root_node || node->d_count > 2) {
		errno = EBUSY;
		return -1;
	}

	if (!node->in_use) {
		g_free_list[++g_free_list_size] = node->index;
		node->type = M_NON;
	}

	g_nodes[node->parent_idx].d_count--;
	node->doomed = 1;
	return 0;
}

static int
imfs_remove_link(Node *node)
{
	if (!node->in_use) {
		g_free_list[++g_free_list_size] = node->index;
		node->type = M_NON;
	}

	node->doomed = 1;
	g_nodes[node->parent_idx].d_count--;
	return 0;
}

static Pipe *
get_pipe(int cage_id, int fd)
{
	FileDesc *fdesc = get_filedesc(cage_id, fd);
	if (fdesc->node->type != M_PIP) {
		return NULL;
	}

	return fdesc->node->p_pipe;
}

static ssize_t
__imfs_pipe_read(int cage_id, int fd, void *buf, size_t count, int pread, off_t offset)
{
	Pipe *_pipe = get_pipe(cage_id, fd);

	LOG("[pipe] [read] offset=%d status=%d\n", count, _pipe->writefd->status);
	while (_pipe->writefd->status && _pipe->offset <= 0) {
	};

	int to_read = _pipe->offset;
	mem_cpy(buf, _pipe->data, to_read);
	_pipe->offset = 0;

	return to_read;
}

static ssize_t
__imfs_read(int cage_id, int fd, void *buf, size_t count, int pread, off_t offset)
{
	FileDesc *c_fd = get_filedesc(cage_id, fd);

	if (fd < 0 || fd >= MAX_FDS || !c_fd->node || !buf || offset < 0) {
		errno = EBADF;
		return -1;
	}

	if (offset < 0) {
		errno = EINVAL;
		return -1;
	}

	Node *node = c_fd->node;

	if (node->type == M_PIP) {
		return __imfs_pipe_read(cage_id, fd, buf, count, pread, offset);
	}

	if (node->type != M_REG) {
		errno = EISDIR;
		return -1;
	}

	if (c_fd->offset >= node->size) {
		return 0;
	}

	size_t available = node->size - c_fd->offset;
	size_t to_read = count < available ? count : available;

	off_t use_offset = pread ? offset : c_fd->offset;

	mem_cpy(buf, node->r_data + use_offset, to_read);
	if (!pread)
		c_fd->offset += to_read;

	return to_read;
}

static ssize_t
__imfs_readv(int cage_id, int fd, const struct iovec *iov, int len, off_t offset, int pread)
{
	int ret, fin = 0;
	for (int i = 0; i < len; i++) {
		ret = __imfs_read(cage_id, fd, iov[i].iov_base, iov[i].iov_len, 0, 0);
		if (ret == -1)
			return ret;
		else
			fin += ret;
	}

	return fin;
}

static ssize_t
__imfs_pipe_write(int cage_id, int fd, const void *buf, size_t count, int pread, off_t offset)
{
	Pipe *_pipe = get_pipe(cage_id, fd);

	mem_cpy(_pipe->data, buf, count);
	_pipe->offset += count;
	LOG("[pipe] offset=%d\n", count);

	return count;
}

ssize_t
imfs_new_read(int cage_id, int fd, const void *buf, size_t count)
{
	FileDesc *fdesc = get_filedesc(cage_id, fd);
	Node *node = fdesc->node;
	int offset = fdesc->offset;

	fprintf(stderr, "[imfs] offset=%d count=%d total_size=%d\n", offset, count, node->total_size);

	if (offset >= node->total_size) return 0;

	if (offset + count > node->total_size)
		count = node->total_size - offset;

	size_t read = 0;
	size_t local_offset = offset;
	Chunk *c = node->r_head;

	while (c && local_offset >= 1024) {
		local_offset -= 1024;
		c = c->next;
	}

	while (read < count && c) {
		size_t available = c->used - local_offset;
		size_t to_copy = count - read;
		if(to_copy > available) to_copy = available;

		mem_cpy(buf + read, c->data + local_offset, to_copy);

		read += to_copy;
		local_offset = 0;
		c = c->next;
	}

	fdesc->offset += read;

	return read;
}

ssize_t 
imfs_new_write(int cage_id, int fd, const void *buf, size_t count)
{

	FileDesc *fdesc = get_filedesc(cage_id, fd);
	Node *node = fdesc->node;
	int offset = fdesc->offset;

	size_t written = 0;

	size_t chunk_offset = 0;
	Chunk *c = node->r_head;
	size_t local_offset = offset;

	while(c && local_offset >= 1024) {
		local_offset -= 1024;
		chunk_offset += c->used;
		if(!c->next) break;
		c = c->next;
	}

	while (written < count) {
		if(!c) {
			Chunk *new_chunk = calloc(1, sizeof(Chunk));
			if(!new_chunk) return -1;
			if(node->r_tail) node->r_tail->next = new_chunk;
			node->r_tail = new_chunk;
			if(!node->r_head) node->r_head = new_chunk;
			c = new_chunk;
		}
	
		size_t space = 1024 - local_offset;
		size_t to_copy = count - written;
		if(to_copy > space) to_copy = space;

		mem_cpy(c->data + local_offset, buf + written, to_copy);


		if(local_offset + to_copy > c->used)
			c->used = local_offset + to_copy;

		written += to_copy;
		local_offset = 0;
		c = c->next;
	}

//	if(offset + count > node->total_size)
		node->total_size = offset + count;

	fdesc->offset += written;

	return written;
}

static ssize_t
__imfs_write(int cage_id, int fd, const void *buf, size_t count, int pread, off_t offset)
{
	if (fd < 0 || fd >= MAX_FDS) {
		errno = EBADF;
		return -1;
	}

	if (offset < 0) {
		errno = EINVAL;
		return -1;
	}

	FileDesc *fdesc = get_filedesc(cage_id, fd);

	Node *node = fdesc->node;

	if (node->type == M_PIP) {
		return __imfs_pipe_write(cage_id, fd, buf, count, pread, offset);
	}

	if (node->type != M_REG) {
		errno = EISDIR;
		return -1;
	}

	size_t new_size = fdesc->offset + count;
	fprintf(stderr, "[imfs] new size= %d\n", new_size);
	if (new_size > node->size) {
		char *new_data = realloc(node->r_data, new_size);
		
		if(!new_data) {
			fprintf(stderr, "realloc failed\n");
			perror("Realloc failed:");
			return -1;
		}

		node->r_data = new_data;
		node->size = new_size;
	}

	off_t use_offset = pread ? offset : fdesc->offset;

	mem_cpy(node->r_data + use_offset, buf, count);

	if (!pread)
		fdesc->offset += count;

	clock_gettime(CLOCK_REALTIME, &node->mtime);

	return count;
}

void
list_all_files()
{
	for(int i = 0; i < g_next_node; i++) {
		fprintf(stderr, "Node: %d Type: %d Name: %s", i, g_nodes[i].type, g_nodes[i].name);
	}
}

static ssize_t
__imfs_writev(int cage_id, int fd, const struct iovec *iov, int count, off_t offset, int pread)
{
	int ret, fin = 0;
	for (int i = 0; i < count; i++) {
		ret = __imfs_write(cage_id, fd, iov[i].iov_base, iov[i].iov_len, pread, count);
		if (ret == -1)
			return ret;
		else
			fin += ret;
	}
	return fin;
}

static int
__imfs_stat(int cage_id, Node *node, struct stat *statbuf)
{
	if (node == NULL)
		return -1;

	*statbuf = (struct stat) {
		.st_dev = GET_DEV,
		.st_ino = node->index,
		.st_mode = node->mode,
		.st_nlink = 1,
		.st_uid = GET_UID,
		.st_gid = GET_GID,
		.st_rdev = 0,
		.st_size = node->size,
		.st_blksize = 512,
		.st_blocks = node->size / 512,
#ifdef __APPLE__
		.st_atimespec = node->atime,
		.st_mtimespec = node->mtime,
		.st_ctimespec = node->ctime,
		.st_birthtimespec = node->btime,
#else
		.st_atim = node->atime,
		.st_mtim = node->mtime,
		.st_ctim = node->ctime,
#endif
	};

	return 0;
}

void
imfs_init(void)
{
	// g_state = mmap(NULL, sizeof(struct IMFState), PROT_READ | PROT_WRITE, MAP_SHARED | MAP_ANONYMOUS, -1, 0);
	g_free_list_size = -1;

	LOG("initing g_fdtable\n");
	for (int cage_id = 0; cage_id < MAX_PROCS; cage_id++) {
		for (int i = 0; i < MAX_FDS; i++) {
			g_fdtable[cage_id][i] = (FileDesc) {
				.node = NULL,
				.offset = 0,
			};
		}
	}

	// g_nodes = mmap(NULL, sizeof(Node) * MAX_NODES, PROT_READ | PROT_WRITE, MAP_SHARED | MAP_ANONYMOUS, -1, 0);
	LOG("initing g_nodes\n");
	for (int i = 0; i < MAX_NODES; i++) {
		g_nodes[i] = (Node) {
			.type = M_NON,
			.index = i,
			.in_use = 0,
			.d_count = 0,
			.size = 0,
			.info = NULL,
			.mode = 0,
		};
	}

	for (int i = 0; i < MAX_PROCS; i++) {
		g_fd_free_list_size[i] = -1;
	}

	for (int i = 0; i < MAX_PROCS; i++) {
		g_next_fd[i] = 3;
	}

	Node *root_node = imfs_create_node("/", M_DIR, 0755);
	root_node->parent_idx = root_node->index;

	Node *dot = imfs_create_node(".", M_LNK, 0);
	if (!dot)
		exit(1);
	dot->l_link = root_node;

	Node *dotdot = imfs_create_node("..", M_LNK, 0);
	if (!dotdot)
		exit(1);

	if (add_child(root_node, dot) != 0)
		exit(1);
	if (add_child(root_node, dotdot) != 0)
		exit(1);
	dotdot->l_link = root_node;

	g_root_node = &g_nodes[0];
}

//
// FS Entrypoints
//

int
imfs_openat(int cage_id, int dirfd, const char *path, int flags, mode_t mode)
{
	if (!path) {
		errno = EINVAL;
		return -1;
	}

	if (dirfd == -1) {
		errno = EBADF;
		return -1;
	}

	int count;
	char namecomp[MAX_DEPTH][MAX_NODE_NAME];

	split_path(path, &count, namecomp);

	char *filename = namecomp[count - 1];

	Node *parent_node;

	parent_node = imfs_find_node_namecomp(cage_id, dirfd, namecomp, count - 1);

	if (!parent_node || parent_node->type != M_DIR) {
		errno = ENOTDIR;
		return -1;
	}

	Node *node = imfs_find_node(cage_id, dirfd, path);

	// New File
	if (!node) {
		if (!(flags & O_CREAT)) {
			errno = ENOENT;
			return -1;
		}

		if (str_len(filename) > MAX_NODE_NAME - 1) {
			errno = ENAMETOOLONG;
			return -1;
		}

		if (str_len(filename) > 64) {
			errno = ENAMETOOLONG;
			return -1;
		}

		node = imfs_create_node(filename, M_REG, mode);
		if (!node) {
			return -1;
		}

		if (add_child(parent_node, node) != 0) {
			errno = ENOMEM;
			node->type = M_NON;
			return -1;
		}
	} else {
		// File Exists
		if (/*flags & O_EXCL ||*/ flags & O_CREAT) {
			errno = EEXIST;
			return -1;
		}

		if (node->type == M_DIR && !(flags & O_DIRECTORY)) {
			errno = EISDIR;
			return -1;
		}

		// Check for file access based on flags and mode.

		switch (O_ACCMODE & flags) {
		case O_RDONLY:
			if (!(node->mode & S_IROTH)) {
				errno = EACCES;
				return -1;
			}
			break;
		case O_RDWR:
			if (!(node->mode & S_IWOTH) || !(node->mode & S_IROTH)) {
				errno = EACCES;
				return -1;
			}
			break;
		case O_WRONLY:
			if (!(node->mode & S_IWOTH)) {
				errno = EACCES;
				return -1;
			}
			break;
		default:
			break;
		}
	}

	return imfs_allocate_fd(cage_id, node, flags);
}

int
imfs_open(int cage_id, const char *path, int flags, mode_t mode)
{
	return imfs_openat(cage_id, AT_FDCWD, path, flags, mode);
}

int
imfs_creat(int cage_id, const char *path, mode_t mode)
{
	return imfs_open(cage_id, path, O_WRONLY | O_CREAT | O_TRUNC, mode);
}

int
imfs_close(int cage_id, int fd)
{
	if (fd < 0 || fd >= MAX_FDS || !g_fdtable[cage_id][fd].node) {
		errno = EBADF;
		return -1;
	}

	FileDesc *fdesc = get_filedesc(cage_id, fd);
	fdesc->node->in_use--;

	if (fdesc->node->doomed) {
		fdesc->node->type = M_NON;
		g_free_list[++g_free_list_size] = fdesc->node->index;
	}

	g_fd_free_list[cage_id][++g_fd_free_list_size[cage_id]] = fd;

	*fdesc = (FileDesc) {
		.node = NULL,
		.offset = 0,
		.status = 0,
	};

	return 0;
}

ssize_t
imfs_write(int cage_id, int fd, const void *buf, size_t count)
{
	return __imfs_write(cage_id, fd, buf, count, 0, 0);
}

ssize_t
imfs_pwrite(int cage_id, int fd, const void *buf, size_t count, off_t offset)
{
	return __imfs_write(cage_id, fd, buf, count, 1, offset);
}

ssize_t
imfs_writev(int cage_id, int fd, const struct iovec *iov, int count)
{
	return __imfs_writev(cage_id, fd, iov, count, 0, 0);
}

ssize_t
imfs_pwritev(int cage_id, int fd, const struct iovec *iov, int count, off_t offset)
{
	return __imfs_writev(cage_id, fd, iov, count, offset, 1);
}

ssize_t
imfs_read(int cage_id, int fd, void *buf, size_t count)
{
	return __imfs_read(cage_id, fd, buf, count, 0, 0);
}

ssize_t
imfs_pread(int cage_id, int fd, void *buf, size_t count, off_t offset)
{
	return __imfs_read(cage_id, fd, buf, count, 1, offset);
}

ssize_t
imfs_readv(int cage_id, int fd, const struct iovec *iov, int count)
{
	return __imfs_readv(cage_id, fd, iov, count, 0, 0);
}

ssize_t
imfs_preadv(int cage_id, int fd, const struct iovec *iov, int count, off_t offset)
{
	return __imfs_readv(cage_id, fd, iov, count, offset, 1);
}

int
imfs_fcntl(int cage_id, int fd, int op, int arg)
{
	FileDesc *fdesc = get_filedesc(cage_id, fd);

	if(!fdesc){
		return -1;
	}

	switch(fd) {
		case F_GETFL:
			return fdesc->flags;
		default:
			return -1;
	}
}

int
imfs_mkdirat(int cage_id, int fd, const char *path, mode_t mode)
{
	if (!path) {
		errno = EINVAL;
		return -1;
	}

	Node *parent;

	char namecomp[MAX_DEPTH][MAX_NODE_NAME];
	int count;

	split_path(path, &count, namecomp);
	char *filename = namecomp[count - 1];

	if (str_compare(filename, ".") || str_compare(filename, "..")) {
		errno = EINVAL;
		return -1;
	}

	parent = imfs_find_node_namecomp(cage_id, fd, namecomp, count - 1);
	if (!parent) {
		errno = EINVAL;
		return -1;
	}

    	Node *node = imfs_find_node_namecomp(cage_id, fd, namecomp, count);
    	if(node) {
        	return 0;
    	}

	node = imfs_create_node(filename, M_DIR, mode);
	
	// Node *node = imfs_create_node(filename, M_DIR, mode);
	if (!node) {
		return -1;
	}

	if (add_child(parent, node) != 0) {
		errno = ENOMEM;
		node->type = M_NON;
		return -1;
	}

	Node *dot = imfs_create_node(".", M_LNK, 0);
	if (!dot)
		return -1;
	dot->l_link = node;

	Node *dotdot = imfs_create_node("..", M_LNK, 0);
	if (!dotdot)
		return -1;

	if (add_child(node, dot) != 0)
		return -1;
	if (add_child(node, dotdot) != 0)
		return -1;

	dotdot->l_link = &g_nodes[node->parent_idx];

	LOG("Created Node: \n");
	LOG("Index: %d \n", node->index);
	LOG("Name: %s\n", node->name);
	LOG("Type: %d\n", node->type);

	return 0;
}

int
imfs_mkdir(int cage_id, const char *path, mode_t mode)
{
	LOG("Making dir. %s | %d \n", path, mode);
	return imfs_mkdirat(cage_id, AT_FDCWD, path, mode);
}

int
imfs_linkat(int cage_id, int olddirfd, const char *oldpath, int newdirfd, const char *newpath, int flags)
{
	Node *oldnode = imfs_find_node(cage_id, olddirfd, oldpath);

	if (!oldnode) {
		errno = EINVAL;
		return -1;
	}

	char namecomp[MAX_DEPTH][MAX_NODE_NAME];
	int count;

	Node *newnode = imfs_find_node(cage_id, newdirfd, newpath);
	if (newnode != NULL) {
		errno = EINVAL;
		return -1;
	}

	split_path(newpath, &count, namecomp);

	char *filename = namecomp[count - 1];

	Node *newnode_parent = imfs_find_node_namecomp(cage_id, newdirfd, namecomp, count - 1);
	newnode = imfs_create_node(filename, M_LNK, 0);

	newnode->l_link = oldnode;

	if (add_child(newnode_parent, newnode) != 0) {
		errno = ENOMEM;
		newnode->type = M_NON;
		return -1;
	}

	clock_gettime(CLOCK_REALTIME, &newnode->ctime);

	return 0;
}

int
imfs_link(int cage_id, const char *oldpath, const char *newpath)
{
	return imfs_linkat(cage_id, AT_FDCWD, oldpath, AT_FDCWD, newpath, 0);
}

int
imfs_symlink(int cage_id, const char *oldpath, const char *newpath)
{
	return imfs_linkat(cage_id, AT_FDCWD, oldpath, AT_FDCWD, newpath, 0);
}

int
imfs_rename(int cage_id, const char *oldpath, const char *newpath)
{
	// TODO
	return 0;
}

int
imfs_chown(int cage_id, const char *pathname, uid_t owner, gid_t group)
{
	// TODO
	Node *node = imfs_find_node(cage_id, AT_FDCWD, pathname);
	clock_gettime(CLOCK_REALTIME, &node->ctime);
	return 0;
}

int
imfs_chmod(int cage_id, const char *pathname, mode_t mode)
{
	Node *node = imfs_find_node(cage_id, AT_FDCWD, pathname);

	if (!node) {
		errno = ENOENT;
		return -1;
	}

	node->mode = (node->mode & ~0777) | mode;

	return 0;
}

int
imfs_fchmod(int cage_id, int fd, mode_t mode)
{
	FileDesc *fdesc = get_filedesc(cage_id, fd);

	if (!fdesc || !fdesc->node) {
		errno = ENOENT;
		return -1;
	}

	fdesc->node->mode = (fdesc->node->mode & ~0777) | mode;

	return 0;
}

int
imfs_remove(int cage_id, const char *pathname)
{
	Node *node = imfs_find_node(cage_id, AT_FDCWD, pathname);

	if (!node) {
		errno = ENOENT;
		return -1;
	}

	// if (node->in_use) {
	// 	errno = EBUSY;
	// 	return -1;
	// }

	switch (node->type) {
	case M_DIR:
		return imfs_remove_dir(node);
	case M_LNK:
		return imfs_remove_link(node);
	case M_REG:
		return imfs_remove_file(node);
	default:
		return 0;
	}
}

int
imfs_rmdir(int cage_id, const char *pathname)
{
	return imfs_remove(cage_id, pathname);
}

int
imfs_unlink(int cage_id, const char *pathname)
{
	return imfs_remove(cage_id, pathname);
}

off_t
imfs_lseek(int cage_id, int fd, off_t offset, int whence)
{
	FileDesc *fdesc = get_filedesc(cage_id, fd);

	if (!fdesc->node) {
		errno = EBADF;
		return -1;
	}

	off_t ret = fdesc->offset;

	// SEEK_HOLE and SEEK_DATA need to be reworked. Unclear as to what it is they do
	switch (whence) {
	case SEEK_SET:
		ret = offset;
		break;
	case SEEK_CUR:
		ret += offset;
		break;
	case SEEK_END:
		ret = fdesc->node->size;
		break;
	case SEEK_HOLE:
		while (*(char *)(fdesc->node + ret)) {
			ret++;
		}
		break;
	case SEEK_DATA:
		while (!*(char *)(fdesc->node + ret)) {
			ret++;
		}
		break;
	default:
		errno = EINVAL;
		return ret - 1;
	}

	fdesc->offset = ret;

	return ret;
}

int
imfs_dup(int cage_id, int fd)
{
	return imfs_dup_fd(cage_id, fd, -1);
}

int
imfs_dup2(int cage_id, int oldfd, int newfd)
{
	return imfs_dup_fd(cage_id, oldfd, newfd);
}

int
imfs_lstat(int cage_id, const char *pathname, struct stat *statbuf)
{
	Node *node = imfs_find_node(cage_id, AT_FDCWD, pathname);
	return __imfs_stat(cage_id, node, statbuf);
}

int
imfs_stat(int cage_id, const char *pathname, struct stat *statbuf)
{
	LOG("cage=%d pathname=%s\n", cage_id, pathname);
	Node *node = imfs_find_node(cage_id, AT_FDCWD, pathname);
	if (!node) {
		errno = ENOENT;
		return -1;
	}
	if (node->type == M_LNK)
		return __imfs_stat(cage_id, node->l_link, statbuf);
	return __imfs_stat(cage_id, node, statbuf);
}

int
imfs_fstat(int cage_id, int fd, struct stat *statbuf)
{
	Node *node = get_filedesc(cage_id, fd)->node;
	if (node->type == M_LNK)
		return __imfs_stat(cage_id, node->l_link, statbuf);
	return __imfs_stat(cage_id, node, statbuf);
}

I_DIR *
imfs_opendir(int cage_id, const char *name)
{
	I_DIR *dirstream = NULL;
	int fd = imfs_open(cage_id, name, O_DIRECTORY, 0);
	
	Node *node = get_filedesc(cage_id, fd)->node;

	LOG("Crash here?\n");

	*dirstream = (I_DIR) {
		.fd = fd,
		.node = node,
		.size = 0,
		.offset = 0,
		.filepos = 0,
	};

	return dirstream;
}

struct dirent *
imfs_readdir(int cage_id, I_DIR *dirstream)
{
	struct dirent *ret = malloc(sizeof(struct dirent));

	Node *dirnode = dirstream->node;

	if (dirstream->offset >= dirnode->d_count) {
		return NULL;
	}

	// Next entry

	struct DirEnt nextentry = dirnode->d_children[dirstream->offset++];

	int ino = nextentry.node->index;
	int _type = nextentry.node->type;
	size_t namelen = str_len(nextentry.name);

	*ret = (struct dirent) {
		.d_ino = ino,	// 8
		.d_reclen = 32, // 24
		// .d_namlen = namelen,  // 32 + X
		.d_type = _type, // 36 + X
	};

	str_ncopy(ret->d_name, nextentry.name, namelen);
	ret->d_name[namelen + 1] = '\0';

	return ret;
}

int
imfs_pipe(int cage_id, int pipefd[2])
{
	Node *pipenode = imfs_create_node("APIP", M_PIP, 0);
	pipefd[0] = imfs_allocate_fd(cage_id, pipenode, 0);
	pipefd[1] = imfs_allocate_fd(cage_id, pipenode, 0);

	pipenode->p_pipe = mmap(NULL, sizeof(Pipe), PROT_READ | PROT_WRITE, MAP_SHARED | MAP_ANONYMOUS, -1, 0);

	pipenode->p_pipe->offset = 0;
	// pipenode->p_pipe->data = "";
	pipenode->p_pipe->readfd = get_filedesc(cage_id, pipefd[0]);
	pipenode->p_pipe->writefd = get_filedesc(cage_id, pipefd[1]);

	return 0;
}

int
imfs_mkfifo(int cage_id, const char *pathname, mode_t mode)
{
	errno = EOPNOTSUPP;
	return -1;
}

int
imfs_mknod(int cage_id, const char *pathname, mode_t mode, dev_t dev)
{
	errno = EOPNOTSUPP;
	return -1;
}

int
imfs_bind(int cage_id, int sockfd, const struct sockaddr *addr, socklen_t length)
{
	errno = EOPNOTSUPP;
	return -1;
}

int
imfs_pathconf(int cage_id, const char *pathname, int name)
{
	return PC_CONSTS[name];
}

int
imfs_fpathconf(int cage_id, int fd, int name)
{
	return PC_CONSTS[name];
}

//
// Main func for local testing.
//
