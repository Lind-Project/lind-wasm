#include <stdio.h>
#include <string.h>
#include <errno.h>
#include <stdlib.h>
#include <dirent.h>
#include <unistd.h>
#include <register_handler.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <sys/stat.h>
#include <cp_data_between_cages.h>

#include <imfs.h>

FILE *fp;

void load_file(char *path) {
	FILE *fp = fopen("preloads.log", "a");

	fprintf(fp, "\n[load_file] loading=%s\n", path);
	
	char split_path[4096];
	strcpy(split_path, path);

	for(char *p = split_path + 1; *p;p++) {
		if(*p == '/') {
			*p = '\0';
			int ret = imfs_mkdir(0, split_path, 0755);
			*p = '/';
			fprintf(fp, "[load_file] mkdir=%d\n", ret);
		}
	}


	int imfs_fd = imfs_open(0, path, O_CREAT | O_WRONLY, 0777);
	fprintf(fp, "[load_file] created file: %s\n", path);

	int fd = open(path, O_RDONLY, 0);
	if(fd < 0) {
		perror("open error");
		return;
	}

	size_t nread;
	while(1) {
		char *buffer = malloc(1024);

		nread = read(fd, buffer, 1024);
		
		if(nread == 0) {
			free(buffer);
			break;
		}
		
		fprintf(fp, "nread=%d   ", nread);
		imfs_write(0, imfs_fd, buffer, nread);

		free(buffer);
	}

	fclose(fp);
	close(fd);
	imfs_close(0, imfs_fd);
}

void load_folder(const char *path) {
	fprintf(stderr, "[load_folder] Loading=%s\n", path);
	struct dirent *entry;
	DIR *dp = opendir(path);

	if(!dp) {
		perror("[load_folder] opendir");
		return;
	}

	while((entry = readdir(dp))) {
		if(strcmp(entry->d_name, ".") == 0 ||
				strcmp(entry->d_name, "..") == 0)
			continue;

		char fullpath[4096];
		snprintf(fullpath, sizeof(fullpath), "%s/%s", path, entry->d_name);

		struct stat st;
		if(stat(fullpath, &st) == -1) {
			perror("[load_folder] stat");
			continue;
		}

		if(S_ISDIR(st.st_mode)) {
			int ret = imfs_mkdir(0, fullpath, 0755);
			fprintf(stderr, "[load_folder] imfs_mkdir %s = %d\n", fullpath, ret);

			load_folder(fullpath);
		} else if (S_ISREG(st.st_mode)) {
			load_file(fullpath);
		} else {
			fprintf(stderr, "[load_folder] skipping %s\n", fullpath);
		}
	}

	closedir(dp);
}

int open_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage){
	int thiscage = getpid();
	char *pathname = malloc(256);
	
	if(pathname == NULL){
		perror("malloc failed");
		exit(EXIT_FAILURE);
	}

	cp_data_between_cages(thiscage, arg1cage, arg1, arg1cage, (uint64_t)pathname, thiscage, 256, 1);
	
	int ifd = imfs_open(cageid, pathname, arg2, arg3); 

	if(ifd < 0) {
		fprintf(stderr,"errno=%d\n", errno);
		
		FILE *failed_opens = fopen("failed_opens.log", "a");
		fprintf(failed_opens, "PATH=%s | RET=%d\n", pathname, errno);
		fclose(failed_opens);
		
		perror("imfs open failed.");
	}
	
	fprintf(stderr, "\nOPEN (cageid=%llu, pathname=%s, flags=%llu, mode=%llu )  = %d\n\n", cageid, pathname, arg2, arg3, ifd);
	return ifd;
}

int close_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage){
	int ret = imfs_close(cageid, arg1);
	fprintf(stderr, "CLOSE (cageid=%llu, fd=%llu)\n", cageid, arg1);
	return ret;
}

off_t lseek_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage){
	int thiscage = getpid();

	int fd = arg1;
	off_t offset = (off_t) arg2;
	int whence = (int) arg3;
	
	off_t ret = imfs_lseek(cageid, fd, offset, whence);
	
	fprintf(stderr, "\nLSEEK (cage_id=%llu, fd=%llu, offset=%llu, whence=%llu) =%ld\n\n", cageid, arg1, arg2, arg3, ret);
	return ret;
}

int read_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage){
	int thiscage = getpid();

	int fd = (int) arg1;
	int count = (size_t) arg3;

	ssize_t ret = 4321;

	char *buf = malloc(count);

	if(buf == NULL) {
		fprintf(stderr, "Malloc failed");
		exit(1);
	}

	ret = imfs_read(cageid, arg1, buf, count);

	fprintf(stderr, "READ: imfs_read done. ARG2=%llu\n", arg2);
	if(arg2 != 0) {
		cp_data_between_cages(
			thiscage, 
			arg2cage, // cageid, 
			(uint64_t) buf, 
			thiscage,
			arg2,
			arg2cage,
			count,
			0 // 1
		);
	} else {
		fprintf(stderr, "READ: buffer is NULL\n");
	}

	fprintf(stderr, "\nREAD (cageid=%llu, fd=%llu, buf=[], count=%d) =%d\n\n", cageid, arg1, count, ret);

	free(buf);

	return ret;
}

int write_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage){
	int thiscage = getpid();
	int count = arg3;
	int ret = 1604;
	
	char *buffer = malloc(256);
	
	if(buffer == NULL) {
		perror("malloc failed.");
		exit(1);
	}

	cp_data_between_cages(thiscage, arg2cage, arg2, arg2cage, (uint64_t) buffer, thiscage, count, 0);

	if(arg1 < 3) {
		int hfd = open("host_write", O_WRONLY | O_APPEND, 0);
		write(hfd, buffer, count);
		close(hfd);
		return count;
	}

	ret = imfs_write(cageid, arg1, buffer, count);

	fprintf(stderr, "\nWRITE(cageid=%llu, fd=%llu, buffer=%s, count=%llu) =%d\n\n", cageid, arg1, buffer, arg3, ret);

	return ret;
}

void preloads() {
	const char *env = getenv("PRELOADS");
	if(!env) {
		fprintf(stderr, "no preloads.\n");
		return;
	}

	fprintf(stderr, "[load_file] ENV: %s\n", env);
	char *list = strdup(env);
	if(!list) {
		return;
	}

	fprintf(stderr, "Loading all files\n");
	char *line = strtok(list, "\n");
	
	FILE *fp = fopen("preloads.log", "a");

	while(line) {
		fprintf(fp, "Loading= %s\n", line);
		
		struct stat st;
		if(stat(line, &st) < 0) {
			line = strtok(NULL, "\n");
			continue;
		}
	
		fprintf(fp, "st_mode: %d REG: %d DIR: %d\n", st.st_mode, S_ISREG(st.st_mode), S_ISDIR(st.st_mode));

		if(strlen(line) > 0) {
			if (S_ISREG(st.st_mode))
				load_file(line);
		}
		fprintf(fp, "Loaded {%s}\n", line);
		line = strtok(NULL, "\n");
	}

	fclose(fp);
	free(list);
}

// Main function will always be same in all grates
int main(int argc, char *argv[]) {
    // Should be at least two inputs (at least one grate file and one cage file)
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_file> <grate_file> <cage_file> [...]\n", argv[0]);
        exit(EXIT_FAILURE);
    }

    int grateid = getpid();

    // Because we assume that all cages are unaware of the existence of grate, cages will not handle the logic of `exec`ing 
    // grate, so we need to handle these two situations separately in grate. 
    // grate needs to fork in two situations: 
    // - the first is to fork and use its own cage; 
    // - the second is when there is still at least one grate in the subsequent command line input. 
    // In the second case, we fork & exec the new grate and let the new grate handle the subsequent process.


    for (int i = 1; i < (argc < 3 ? argc : 3); i++) {
        pid_t pid = fork();
	fprintf(stderr, "\n\ni=%d pid=%d\n", i, pid);
        if (pid < 0) {
            perror("fork failed");
            exit(EXIT_FAILURE);
        } else if (pid == 0) {
            if (i % 2 != 0) {
                int cageid = getpid();
		int ret;

		fprintf(stderr, "Sleeping for 3\n");
		sleep(3);

		fprintf(stderr, "Starting Registrations\n");
		
		ret = register_handler(cageid, 2, 0, grateid);
                fprintf(stderr, "open register ret=%d\n", ret);
	    
		ret = register_handler(cageid, 8, 1, grateid);
		fprintf(stderr, "lseek register ret=%d\n", ret);
	    
		ret = register_handler(cageid, 0, 2, grateid);
		fprintf(stderr, "read register ret=%d\n", ret);
	    
	    	ret = register_handler(cageid, 1, 3, grateid);
		fprintf(stderr, "write register ret=%d\n", ret);

		ret = register_handler(cageid, 3, 4, grateid);
		fprintf(stderr, "close register ret=%d\n", ret);
	    }

	    fprintf(stderr,"\n\n---Execing argv[i]=%s---\n\n", argv[i]);
	    char *tccargs[] = {"tcc.wasm", "nodeps.c", "-o", "tccgrateout", NULL};
	    if ( execv(argv[i], tccargs) == -1) {
                perror("execv failed");
                exit(EXIT_FAILURE);
            }
        } else {
  	    printf("Enter sandman\n");
    	    printf("\nargc: %d argv[1]:%s argv[2]=%s\n\n", argc, argv[1], argv[2]);
    	    imfs_init();
    	    fprintf(stderr, "\nimfs_init() called.\n\n");
    	    preloads();
	}
    }

    int status;
    int w;
    while(1) {
    	w = wait(&status);
	if (w > 0) { 
		printf("[Grate] terminated, status: %d\n", status);
		break;
	} else if (w < 0) {
		perror("[Grate] [Wait]");
	}
    }
   
    return 0;
}
