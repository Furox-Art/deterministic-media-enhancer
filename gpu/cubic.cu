// Deterministic integer registration, cubic and local fusion.
#include <cuda_runtime.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
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
__global__ void luminance(const unsigned char*rgb,unsigned char*y,int pixels){
 int i=blockIdx.x*blockDim.x+threadIdx.x;if(i<pixels)y[i]=(77*rgb[i*3]+150*rgb[i*3+1]+29*rgb[i*3+2]+128)/256;
}
__device__ int sad(const unsigned char*a,const unsigned char*b,int w,int x0,int y0,int x1,int y1,int dx,int dy){
 int sum=0,n=0;
 for(int y=y0;y<y1;y+=2)for(int x=x0;x<x1;x+=2){
   // Halo bounds ensure all half-pixel indices and their neighbors are valid.
   int hx=2*x+dx,hy=2*y+dy,xx=hx/2,yy=hy/2,u=hx%2,v=hy%2;
   int i=yy*w+xx;
   int pred=((2-u)*(2-v)*a[i]+u*(2-v)*a[i+1]+(2-u)*v*a[i+w]+u*v*a[i+w+1]+2)/4;
   sum+=abs((int)b[y*w+x]-pred);n++;
 }
 return 100*sum/n;
}
__global__ void registration(const unsigned char*center,const unsigned char*neighbors,signed char*out,int w,int h,int n){
 // One block per tile, neighbor and direction; all ties preserve the CPU's enumeration order.
 int work=blockIdx.x/2,rev=blockIdx.x%2,tile=work/n,bid=work%n,tid=threadIdx.x;
 int tiles_x=(w+15)/16,tx=(tile%tiles_x)*16,ty=(tile/tiles_x)*16;
 int x0=max(tx-4,5),y0=max(ty-4,5),x1=min(tx+20,w-5),y1=min(ty+20,h-5);
 signed char*result=out+blockIdx.x*3;
 if(x1-x0<8||y1-y0<8){if(!tid){result[0]=0;result[1]=0;result[2]=0;}return;}
 const unsigned char*other=neighbors+bid*w*h;
 const unsigned char*a=rev?other:center,*b=rev?center:other;
 __shared__ int coarse[81],fine[9],bx,by,best,zero;
 if(tid<81)coarse[tid]=sad(a,b,w,x0,y0,x1,y1,2*(tid%9)-8,2*(tid/9)-8);
 __syncthreads();
 if(!tid){zero=coarse[40];best=zero;bx=by=0;for(int k=0;k<81;k++)if(coarse[k]<best){best=coarse[k];bx=2*(k%9)-8;by=2*(k/9)-8;}}
 __syncthreads();
 if(tid<9)fine[tid]=sad(a,b,w,x0,y0,x1,y1,bx+tid%3-1,by+tid/3-1);
 __syncthreads();
 if(!tid){
   int dx=bx,dy=by;
   for(int k=0;k<9;k++)if(fine[k]<best){best=fine[k];dx=bx+k%3-1;dy=by+k/3-1;}
   int alt=2147483647;for(int k=0;k<81;k++)if(max(abs(2*(k%9)-8-dx),abs(2*(k/9)-8-dy))>=2)alt=min(alt,coarse[k]);
   result[0]=dx;result[1]=dy;result[2]=(best<=600&&zero-best>=25&&alt-best>=25&&(dx%2||dy%2));
 }
}
__global__ void consistency(const signed char*raw,signed char*shifts,int count){
 int i=blockIdx.x*blockDim.x+threadIdx.x;if(i>=count)return;
 const signed char*a=raw+6*i,*b=a+3;
 shifts[3*i]=a[0];shifts[3*i+1]=a[1];shifts[3*i+2]=a[2]&&b[2]&&abs(a[0]+b[0])<=1&&abs(a[1]+b[1])<=1;
}
__global__ void fusion(const unsigned char*base,const unsigned char*neighbors,const signed char*shifts,unsigned char*out,unsigned char*flags,int w,int h,int n){
 int p=blockIdx.x*blockDim.x+threadIdx.x;if(p>=4*w*h)return;
 int x=p%(2*w),y=p/(2*w),phase=x%2+2*(y%2),sum[3]={0,0,0},count=0;
 int k=p*3,tiles=(w+15)/16,tile=(y/32)*tiles+x/32;
 if(phase)for(int b=0;b<n;b++){
   const signed char*s=shifts+3*(tile*n+b);if(!s[2])continue;
   int hx=x-s[0],hy=y-s[1];if(hx<0||hy<0||hx>=2*w||hy>=2*h||hx%2||hy%2)continue;
   int nx=hx/2,ny=hy/2;if(nx<2||ny<2||nx>=w-2||ny>=h-2)continue;
   const unsigned char*a=neighbors+b*w*h*3;int off=(ny*w+nx)*3;
   bool ok=true;for(int c=0;c<3;c++)if(abs((int)a[off+c]-base[k+c])>18)ok=false;
   if(!ok)continue;
   int bias[3]={0,0,0},err[3]={0,0,0};
   for(int j=-2;j<=2&&ok;j++)for(int i=-2;i<=2;i++){
     if(!i&&!j)continue;
     int tx=2*(nx+i)+s[0],ty=2*(ny+j)+s[1];
     if(tx<0||ty<0||tx>=2*w||ty>=2*h){ok=false;break;}
     for(int c=0;c<3;c++){int d=(int)a[((ny+j)*w+nx+i)*3+c]-base[(ty*2*w+tx)*3+c];bias[c]+=d;err[c]+=abs(d);}
   }
   for(int c=0;c<3;c++)if(abs(bias[c])>120||err[c]>144)ok=false;
   if(ok){for(int c=0;c<3;c++)sum[c]+=a[off+c];count++;}
 }
 for(int c=0;c<3;c++)out[k+c]=(base[k+c]+2*sum[c]+count)/(1+2*count);
 flags[p]=count?phase:0;
}
int main(int argc,char**argv){
 if(argc!=4||strcmp(argv[3],"fusion-v2"))return 2;
 char *e1,*e2;long w=strtol(argv[1],&e1,10),h=strtol(argv[2],&e2,10);
 if(*e1||*e2||w<32||h<32||w>2073600||h>2073600||w*h>2073600)return 2;
 int n=w*h*3,m=n*4,tiles=((w+15)/16)*((h+15)/16);
 std::vector<unsigned char>a(n),b(m),neighbors(n*4),flags(m/3);
 unsigned char *da,*db,*dn,*dout,*df,*dy,*dyn;signed char *ds,*dr;
 CHECK(cudaMalloc(&da,n));CHECK(cudaMalloc(&db,m));CHECK(cudaMalloc(&dn,n*4));CHECK(cudaMalloc(&dout,m));CHECK(cudaMalloc(&df,m/3));CHECK(cudaMalloc(&ds,tiles*12));CHECK(cudaMalloc(&dr,tiles*24));CHECK(cudaMalloc(&dy,w*h));CHECK(cudaMalloc(&dyn,w*h*4));
 for(;;){
   int k=fgetc(stdin);if(k==EOF&&feof(stdin))break;if(k<0||k>4)return 3;
   if(fread(a.data(),1,n,stdin)!=(size_t)n||fread(neighbors.data(),1,k*n,stdin)!=(size_t)(k*n))return 3;
   CHECK(cudaMemcpy(da,a.data(),n,cudaMemcpyHostToDevice));
   if(k){
     CHECK(cudaMemcpy(dn,neighbors.data(),k*n,cudaMemcpyHostToDevice));
     luminance<<<(w*h+255)/256,256>>>(da,dy,w*h);CHECK(cudaGetLastError());
     luminance<<<(k*w*h+255)/256,256>>>(dn,dyn,k*w*h);CHECK(cudaGetLastError());
     registration<<<tiles*k*2,128>>>(dy,dyn,dr,w,h,k);CHECK(cudaGetLastError());
     consistency<<<(tiles*k+255)/256,256>>>(dr,ds,tiles*k);CHECK(cudaGetLastError());
   }
   cubic<<<(m+255)/256,256>>>(da,db,w,h);CHECK(cudaGetLastError());
   fusion<<<(m/3+255)/256,256>>>(db,dn,ds,dout,df,w,h,k);CHECK(cudaGetLastError());CHECK(cudaDeviceSynchronize());
   CHECK(cudaMemcpy(b.data(),dout,m,cudaMemcpyDeviceToHost));CHECK(cudaMemcpy(flags.data(),df,m/3,cudaMemcpyDeviceToHost));
   if(fwrite(b.data(),1,m,stdout)!=(size_t)m||fwrite(flags.data(),1,m/3,stdout)!=(size_t)(m/3)||fflush(stdout))return 4;
 }
 CHECK(cudaFree(da));CHECK(cudaFree(db));CHECK(cudaFree(dn));CHECK(cudaFree(dout));CHECK(cudaFree(df));CHECK(cudaFree(ds));CHECK(cudaFree(dr));CHECK(cudaFree(dy));CHECK(cudaFree(dyn));return 0;
}
