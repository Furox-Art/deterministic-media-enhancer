#include "../src/analysis.hpp"
#include <iostream>
#include <vector>

int main(){
    constexpr int w=128,h=128;
    std::vector<unsigned char> p(w*h,128);
    auto check=[](bool ok,const char* msg){if(!ok)std::cerr<<msg<<'\n';return ok;};
    bool ok=true;
    auto m=dme::measure(p.data(),w,h);
    ok &= check(m.noise==0 && m.edge==0 && m.block==100,"flat image metrics");
    ok &= check(dme::plan(m,"2").find("hqdn3d")==std::string::npos,"flat image should not denoise");
    for(int y=0;y<h;++y)for(int x=0;x<w;++x)
        p[y*w+x]=static_cast<unsigned char>(128+((x+y)%2?4:-4));
    m=dme::measure(p.data(),w,h);
    ok &= check(m.noise>=350,"high-frequency residual metric");
    ok &= check(dme::plan(m,"2").find("hqdn3d=2.5")!=std::string::npos,"high residual denoise plan");
    for(int y=0;y<h;++y)for(int x=0;x<w;++x)
        p[y*w+x]=static_cast<unsigned char>(((x/8+y/8)%2)?96:160);
    m=dme::measure(p.data(),w,h);
    ok &= check(m.block>=145 && m.edge>=24,"block boundary metric");
    ok &= check(dme::plan(m,"2").find("deblock")!=std::string::npos,"block artifact plan");
    for(int y=0;y<h;++y)for(int x=0;x<w;++x)
        p[y*w+x]=static_cast<unsigned char>(std::clamp(128+(x-64)*16,0,255));
    m=dme::measure(p.data(),w,h);
    ok &= check(m.edge>=24 && m.curvature<55,"soft edge metric");
    ok &= check(dme::plan(m,"2").find("unsharp")!=std::string::npos,"soft edge plan");
    return ok?0:1;
}
