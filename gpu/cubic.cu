#include <cuda_runtime.h>
#include <cstdio>
#include <cstdlib>
#include <vector>
#define CHECK(x) do{auto e=(x);if(e!=cudaSuccess){fprintf(stderr,"%s: %s\n",#x,cudaGetErrorString(e));return 1;}}while(0)
__device__ int clip(int x,int hi){return x<0?0:x>hi?hi:x;}
__global__ void cubic(const unsigned char* a,unsigned char* b,int w,int h){
 int p=blockIdx.x*blockDim.x+threadIdx.x;if(p>=12*w*h)return;
 int c=p%3,x=(p/3)%(2*w),y=p/(6*w),s=0;
 int half[]={-1,9,9,-1},exact[]={0,16,0,0};
 for(int j=0;j<4;j++)for(int i=0;i<4;i++)
 s+=(x%2?half[i]:exact[i])*(y%2?half[j]:exact[j])*a[(clip(y/2+j-1,h-1)*w+clip(x/2+i-1,w-1))*3+c];
 b[p]=clip((s+128)/256,255);
}
int main(int argc,char**argv){
 if(argc!=3)return 2;
 char *e1,*e2;long w=strtol(argv[1],&e1,10),h=strtol(argv[2],&e2,10);
 if(*e1||*e2||w<32||h<32||w>2073600||h>2073600||w*h>2073600)return 2;
 int n=w*h*3,m=n*4;std::vector<unsigned char>a(n),b(m);unsigned char *da,*db;
 CHECK(cudaMalloc(&da,n));CHECK(cudaMalloc(&db,m));
 for(;;){
   size_t count=fread(a.data(),1,n,stdin);if(!count&&feof(stdin))break;if(count!=(size_t)n)return 3;
   CHECK(cudaMemcpy(da,a.data(),n,cudaMemcpyHostToDevice));
   cubic<<<(m+255)/256,256>>>(da,db,w,h);CHECK(cudaGetLastError());CHECK(cudaDeviceSynchronize());
   CHECK(cudaMemcpy(b.data(),db,m,cudaMemcpyDeviceToHost));
   if(fwrite(b.data(),1,m,stdout)!=(size_t)m||fflush(stdout))return 4;
 }
 CHECK(cudaFree(da));CHECK(cudaFree(db));return 0;
}
