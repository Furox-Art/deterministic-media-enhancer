#include "../src/output_guard.hpp"
#include <iostream>
int main(){
    constexpr int w=720,h=520;
    dme::Frame a{w,h,std::vector<unsigned char>(w*h,128)},b=a;
    auto g=dme::validate_full(a,b);
    if(!g.accepted||g.tiles!=9)return 1;
    for(int y=450;y<h;++y)for(int x=650;x<w;++x)b.pixels[y*w+x]=255;
    g=dme::validate_full(a,b);
    if(g.accepted||g.rejected==0||g.max_clipping==0)return 2;
    b=a;b.w--;
    try{dme::validate_full(a,b);return 3;}catch(const std::runtime_error&){}
    std::cout<<"Full-frame guard detects off-center damage and geometry mismatch.\n";
}
