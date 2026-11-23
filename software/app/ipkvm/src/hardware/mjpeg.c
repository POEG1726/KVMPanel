/*
 * mjpeg.c
 * Created on Nov 23, 2026
 * Author: @POEG1726
 */

#include <errno.h>
#include <fcntl.h>
#include <linux/videodev2.h>
#include <poll.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>

struct buffer {
    unsigned char *data;
    size_t length;
    int streaming;
};

struct mjpeg {
    int fd;
    size_t width, height, stride, rows, input_size;
    struct buffer input, output;
};

static int control(int fd, unsigned long request, void *arg)
{
    int ret;

    do {
        ret = ioctl(fd, request, arg);
    } while (ret < 0 && errno == EINTR);
    return ret;
}

static void release_buffer(int fd, enum v4l2_buf_type type, struct buffer *buffer)
{
    if (buffer->streaming)
        control(fd, VIDIOC_STREAMOFF, &type);
    if (buffer->data)
        munmap(buffer->data, buffer->length);
}

/** Release both streaming queues and the device. */
void kvmd_mjpeg_close(struct mjpeg *enc)
{
    if (!enc)
        return;
    if (enc->fd >= 0) {
        release_buffer(enc->fd, V4L2_BUF_TYPE_VIDEO_OUTPUT, &enc->input);
        release_buffer(enc->fd, V4L2_BUF_TYPE_VIDEO_CAPTURE, &enc->output);
        close(enc->fd);
    }
    free(enc);
}

static int allocate(int fd, enum v4l2_buf_type type, struct buffer *buffer)
{
    struct v4l2_requestbuffers req = {
        .count = 1, .type = type, .memory = V4L2_MEMORY_MMAP,
    };
    struct v4l2_buffer buf = {.type = type, .memory = V4L2_MEMORY_MMAP};
    void *addr;

    if (control(fd, VIDIOC_REQBUFS, &req) < 0)
        return -1;
    if (!req.count) {
        errno = ENOMEM;
        return -1;
    }
    if (control(fd, VIDIOC_QUERYBUF, &buf) < 0)
        return -1;
    addr = mmap(NULL, buf.length, PROT_READ | PROT_WRITE, MAP_SHARED, fd, buf.m.offset);
    if (addr == MAP_FAILED)
        return -1;
    buffer->data = addr;
    buffer->length = buf.length;
    return 0;
}

/** Configure a single-planar V4L2 JPEG encoder for NV12 input. */
struct mjpeg *kvmd_mjpeg_open(const char *path, unsigned width, unsigned height, unsigned quality)
{
    struct v4l2_format input = {
        .type = V4L2_BUF_TYPE_VIDEO_OUTPUT,
        .fmt.pix = { .width = width, .height = height, .pixelformat = V4L2_PIX_FMT_NV12,
                     .field = V4L2_FIELD_NONE },
    };
    struct v4l2_format output = {
        .type = V4L2_BUF_TYPE_VIDEO_CAPTURE,
        .fmt.pix = { .width = width, .height = height, .pixelformat = V4L2_PIX_FMT_JPEG,
                     .field = V4L2_FIELD_NONE },
    };
    struct v4l2_control jpeg = {.id = V4L2_CID_JPEG_COMPRESSION_QUALITY, .value = quality};
    struct v4l2_capability cap = {0};
    struct mjpeg *enc = calloc(1, sizeof(*enc));
    uint32_t caps;
    int saved;

    if (!enc)
        return NULL;
    enc->fd = open(path, O_RDWR | O_CLOEXEC | O_NONBLOCK);
    if (enc->fd < 0 || control(enc->fd, VIDIOC_QUERYCAP, &cap) < 0)
        goto error;
    caps = cap.capabilities & V4L2_CAP_DEVICE_CAPS ? cap.device_caps : cap.capabilities;
    if ((caps & (V4L2_CAP_VIDEO_M2M | V4L2_CAP_STREAMING)) !=
        (V4L2_CAP_VIDEO_M2M | V4L2_CAP_STREAMING)) {
        errno = ENOTSUP;
        goto error;
    }
    if (control(enc->fd, VIDIOC_S_FMT, &input) < 0 ||
        control(enc->fd, VIDIOC_S_FMT, &output) < 0 ||
        control(enc->fd, VIDIOC_S_CTRL, &jpeg) < 0)
        goto error;
    if (input.fmt.pix.pixelformat != V4L2_PIX_FMT_NV12 ||
        output.fmt.pix.pixelformat != V4L2_PIX_FMT_JPEG ||
        input.fmt.pix.width != width || input.fmt.pix.height != height ||
        output.fmt.pix.width != width || output.fmt.pix.height != height ||
        input.fmt.pix.bytesperline != ((width + 15) & ~15U)) {
        errno = ENOTSUP;
        goto error;
    }
    enc->width = width;
    enc->height = height;
    enc->stride = input.fmt.pix.bytesperline;
    /* Cedrus places the interleaved UV plane after the padded luma height. */
    enc->rows = (height + 15) & ~15U;
    enc->input_size = enc->stride * enc->rows * 3 / 2;
    if (input.fmt.pix.sizeimage != enc->input_size) {
        errno = ENOTSUP;
        goto error;
    }
    if (allocate(enc->fd, input.type, &enc->input) < 0 ||
        allocate(enc->fd, output.type, &enc->output) < 0)
        goto error;
    if (enc->input.length < enc->input_size) {
        errno = EIO;
        goto error;
    }
    return enc;

error:
    saved = errno;
    kvmd_mjpeg_close(enc);
    errno = saved;
    return NULL;
}

/** Convert the CSI YU12 planes to Cedrus NV12, extending the edges to the next macroblock. */
static void pack_nv12(struct mjpeg *enc, const unsigned char *raw, size_t src_stride)
{
    size_t y_size = src_stride * enc->height;
    const unsigned char *u = raw + y_size;
    const unsigned char *v = u + y_size / 4;
    unsigned char *dst = enc->input.data;
    size_t width = enc->width, stride = enc->stride;

    for (size_t y = 0; y < enc->rows; ++y) {
        size_t src_y = y < enc->height ? y : enc->height - 1;
        memcpy(dst, raw + src_y * src_stride, width);
        memset(dst + width, dst[width - 1], stride - width);
        dst += stride;
    }
    for (size_t y = 0; y < enc->rows / 2; ++y) {
        size_t src_y = y < enc->height / 2 ? y : enc->height / 2 - 1;
        size_t row = src_y * src_stride / 2;
        for (size_t x = 0; x < stride / 2; ++x) {
            size_t src_x = x < width / 2 ? x : width / 2 - 1;
            dst[2 * x] = u[row + src_x];
            dst[2 * x + 1] = v[row + src_x];
        }
        dst += stride;
    }
}

static int dequeue(int fd, short event, struct v4l2_buffer *buf)
{
    struct pollfd ready = {.fd = fd, .events = event};
    int ret = poll(&ready, 1, 2500);

    if (ret == 0) {
        errno = ETIMEDOUT;
        return -1;
    }
    if (ret < 0 || control(fd, VIDIOC_DQBUF, buf) < 0)
        return -1;
    if (buf->index || (buf->flags & V4L2_BUF_FLAG_ERROR)) {
        errno = EIO;
        return -1;
    }
    return 0;
}

/** Encode one frame. The returned JPEG is valid until the next call or close. */
int kvmd_mjpeg_encode(struct mjpeg *enc, const unsigned char *raw, size_t length,
                      const unsigned char **jpeg, size_t *size)
{
    struct v4l2_buffer input = {
        .type = V4L2_BUF_TYPE_VIDEO_OUTPUT, .memory = V4L2_MEMORY_MMAP,
        .bytesused = enc->input_size, .field = V4L2_FIELD_NONE,
    };
    struct v4l2_buffer output = {
        .type = V4L2_BUF_TYPE_VIDEO_CAPTURE, .memory = V4L2_MEMORY_MMAP,
    };
    size_t stride = (enc->width + 7) & ~(size_t)7;
    const unsigned char *data = enc->output.data;

    if (length != stride * enc->height * 3 / 2) {
        errno = EINVAL;
        return -1;
    }
    pack_nv12(enc, raw, stride);
    if (control(enc->fd, VIDIOC_QBUF, &output) < 0 ||
        control(enc->fd, VIDIOC_QBUF, &input) < 0)
        return -1;
    if (!enc->output.streaming) {
        if (control(enc->fd, VIDIOC_STREAMON, &output.type) < 0)
            return -1;
        enc->output.streaming = 1;
        if (control(enc->fd, VIDIOC_STREAMON, &input.type) < 0)
            return -1;
        enc->input.streaming = 1;
    }
    if (dequeue(enc->fd, POLLIN, &output) < 0 ||
        dequeue(enc->fd, POLLOUT, &input) < 0)
        return -1;
    if (output.bytesused < 4 || output.bytesused > enc->output.length ||
        data[0] != 0xff || data[1] != 0xd8 ||
        data[output.bytesused - 2] != 0xff || data[output.bytesused - 1] != 0xd9) {
        errno = EIO;
        return -1;
    }
    *jpeg = data;
    *size = output.bytesused;
    return 0;
}
