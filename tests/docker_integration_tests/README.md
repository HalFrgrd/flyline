# Ubuntu/glibc Compatibility Test Results

## Test Results Summary

| Ubuntu Version | glibc Version | flyline Compatibility | Test Result |
|---------------|---------------|----------------------|-------------|
| 18.04         | 2.27          | ❌ INCOMPATIBLE      | FAILS (Expected) |
| 20.04         | 2.31          | ✅ COMPATIBLE        | PASSES |
| 22.04         | 2.35          | ✅ COMPATIBLE        | PASSES |
| 24.04         | 2.39          | ✅ COMPATIBLE        | PASSES |

## Library Requirements
- **flyline built with glibc 2.31** requires minimum glibc 2.29
- **Maximum required version**: GLIBC_2.30

## Compatibility
- ✅ **Ubuntu 20.04+**: Fully compatible
- ❌ **Ubuntu 18.04**: Incompatible - would need to build against glibc 2.27 for support

## Code Reuse Strategy

### 1. Template-based Approach (Recommended)
```dockerfile
# Single template file with ARG
ARG UBUNTU_VERSION=22.04
FROM ubuntu:${UBUNTU_VERSION}
# ... common setup code
```

**Usage:**
```bash
docker build --build-arg UBUNTU_VERSION=20.04 -f Dockerfile.ubuntu.template -t test-ubuntu2004 .
```

### 2. Generated Dockerfiles
```bash
./generate_dockerfiles.sh  # Creates specific files for each version
```

### 3. Individual Files with Shared Content
- Each version has its own file but shares 95% of the code
- Easier for debugging specific versions
- More explicit but less DRY

## Benefits of Current Setup
✅ **Automatic compatibility testing** across Ubuntu versions  
✅ **Clear compatibility matrix** showing which systems are supported  
✅ **Code reuse** through templates and build args  
✅ **Realistic deployment testing** using pre-built glibc-compatible binaries  