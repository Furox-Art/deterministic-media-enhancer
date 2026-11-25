#include "../src/video.hpp"
#include <iostream>
int main(){
    dme::Timeline a{160,120,{0,41667,83333,125000},41667};
    if(!dme::cfr(a)||dme::target_frames(a,60)!=10)return 1;
    dme::Timeline b{320,240,{},16667};
    for(int i=0;i<10;++i)b.pts.push_back(i*1000000/60);
    dme::validate_video(a,b,2,60);
    b.pts.pop_back();
    try{dme::validate_video(a,b,2,60);return 2;}catch(const std::runtime_error&){}
    b.pts.push_back(150000);b.pts[4]+=9000;
    try{dme::validate_video(a,b,2,60);return 3;}catch(const std::runtime_error&){}
    a.pts[2]+=10000;
    if(dme::cfr(a))return 4;
    try{dme::interpolate_chain(a,60);return 5;}catch(const std::runtime_error&){}
    if(dme::timestamp_us("-0.125000")!=-125000)return 6;
    std::cout<<"Timeline rejects frame loss, cadence errors and VFR interpolation.\n";
}
