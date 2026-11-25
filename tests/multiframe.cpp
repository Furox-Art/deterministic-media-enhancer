#include "../src/multiframe.hpp"
#include <cmath>
#include <iostream>
static int truth(int x,int y){
    return static_cast<int>(std::lround(128+43*std::sin(x*0.24)+30*std::cos(y*0.19)+17*std::sin((x+y)*0.45)));
}
static dme::RGBFrame make(int dx,int dy){
    dme::RGBFrame f(96,80);
    for(int y=0;y<f.h;++y)for(int x=0;x<f.w;++x)for(int c=0;c<3;++c)
        f.pixels[(y*f.w+x)*3+c]=static_cast<unsigned char>(truth(2*x+dx,2*y+dy));
    return f;
}
int main(){
    auto center=make(0,0),a=make(1,0),b=make(0,1);
    auto s=dme::register_frame(center,a);
    if(!s.accepted||s.dx!=1||s.dy!=0){std::cerr<<s.dx<<","<<s.dy<<" "<<s.cost<<"\n";return 1;}
    dme::FusionStats plain,merged;
    auto baseline=dme::fuse(center,nullptr,nullptr,plain);
    auto enhanced=dme::fuse(center,&a,&b,merged);
    if(merged.neighbors!=2||merged.samples==0)return 2;
    auto lanczos=[&](int x,int y){
        auto kernel=[](double t){if(t==0)return 1.0;if(std::abs(t)>=4)return 0.0;
            constexpr double pi=3.141592653589793;
            return std::sin(pi*t)*std::sin(pi*t/4)/(pi*pi*t*t/4);};
        double sum=0,weight=0;
        for(int j=y/2-3;j<=y/2+4;++j)for(int i=x/2-3;i<=x/2+4;++i){
            double k=kernel(x/2.0-i)*kernel(y/2.0-j);
            sum+=k*center.at(i,j,0);weight+=k;
        }
        return static_cast<int>(std::lround(sum/weight));
    };
    std::int64_t before=0,after=0,lanczos_error=0;
    for(int y=8;y<enhanced.h-8;++y)for(int x=8;x<enhanced.w-8;++x){
        int p=baseline.at(x,y,0)-truth(x,y),q=enhanced.at(x,y,0)-truth(x,y);
        before+=p*p;after+=q*q;int l=lanczos(x,y)-truth(x,y);lanczos_error+=l*l;
        if(x%2==0&&y%2==0&&enhanced.at(x,y,0)!=center.at(x/2,y/2,0))return 3;
    }
    std::cout<<"Squared error (interior): "<<before<<" -> "<<after<<"; aligned Lanczos4: "<<lanczos_error<<"; fused pixels: "<<merged.samples<<"\n";
    if(after>=before)return 4;
    dme::RGBFrame cut(96,80);std::fill(cut.pixels.begin(),cut.pixels.end(),255);
    dme::FusionStats reject;
    auto safe=dme::fuse(center,&cut,nullptr,reject);
    if(reject.neighbors!=0||safe.pixels!=baseline.pixels)return 5;
    dme::FusionStats repeat;
    if(dme::fuse(center,&a,&b,repeat).pixels!=enhanced.pixels)return 6;
}
