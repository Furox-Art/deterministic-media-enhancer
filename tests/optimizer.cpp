#include "../src/optimizer.hpp"
#include <iostream>

int main(){
    constexpr int w=128,h=128;
    dme::Frame a{w,h,std::vector<unsigned char>(w*h,128)}, b=a;
    bool ok=true;
    auto check=[&](bool x,const char* msg){if(!x){std::cerr<<msg<<'\n';ok=false;}};
    auto metrics=[&](){return dme::measure(a.pixels.data(),w,h);};
    auto c=dme::compare(a,b,metrics());
    check(c.accepted&&c.gain==0,"identity baseline");
    for(int y=0;y<h;++y)for(int x=0;x<w;++x)
        a.pixels[y*w+x]=static_cast<unsigned char>(128+((x+y)%2?4:-4));
    c=dme::compare(a,b,metrics());
    check(c.accepted&&c.gain>15&&c.noise_delta>0,"noise reduction accepted");
    for(int y=0;y<h;++y)for(int x=0;x<w;++x)
        a.pixels[y*w+x]=static_cast<unsigned char>(x<64?80:176);
    c=dme::compare(a,b,metrics());
    check(!c.accepted&&c.edge_loss>12,"edge loss rejected");
    b=a;std::fill(b.pixels.begin(),b.pixels.end(),255);
    c=dme::compare(a,b,metrics());
    check(!c.accepted&&c.clipping>50,"clipping rejected");
    auto list=dme::candidates(metrics());
    check(!list.empty()&&list.front().second.empty(),"baseline always first");
    return ok?0:1;
}
