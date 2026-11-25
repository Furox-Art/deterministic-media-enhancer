#pragma once
#include "analysis.hpp"
#include "restoration.hpp"
#include <map>
#include <numeric>
#include <limits>

namespace dme {
struct Timeline {
    int width=0,height=0;
    std::vector<std::int64_t> pts;
    std::int64_t tail=0;
    std::int64_t duration()const{return pts.back()-pts.front()+tail;}
};
inline std::int64_t timestamp_us(const std::string& s){
    if(s.empty()||s=="N/A")throw std::runtime_error("Missing video timestamp");
    size_t pos=s.find('.');
    std::string whole=s.substr(0,pos),frac=pos==std::string::npos?"":s.substr(pos+1);
    if(frac.size()>6)frac.resize(6);
    frac.append(6-frac.size(),'0');
    const auto sec=std::stoll(whole);
    if(sec>1000000000||sec< -1000000000)throw std::runtime_error("Timestamp out of range");
    return sec*1000000+(s.front()=='-'?-1:1)*std::stoll(frac);
}
inline Timeline inspect_video(const std::string& path){
    auto data=capture({"ffprobe","-v","error","-protocol_whitelist","file,pipe","-threads","1",
        "-select_streams","v:0","-show_entries","frame=best_effort_timestamp_time,pkt_duration_time,width,height",
        "-of","compact=p=0:nk=0",path},64ULL*1024*1024);
    std::istringstream input(std::string(data.begin(),data.end()));
    std::string line,part;
    Timeline t;
    while(std::getline(input,line)){
        std::map<std::string,std::string> fields;
        std::istringstream row(line);
        while(std::getline(row,part,'|')){
            auto k=part.find('=');if(k!=std::string::npos)fields.emplace(part.substr(0,k),part.substr(k+1));
        }
        if(!fields.count("best_effort_timestamp_time"))continue;
        auto pts=timestamp_us(fields.at("best_effort_timestamp_time"));
        int w=std::stoi(fields.at("width")),h=std::stoi(fields.at("height"));
        if(w<=0||h<=0)throw std::runtime_error("Invalid video dimensions");
        if(t.pts.empty()){t.width=w;t.height=h;}
        if(w!=t.width||h!=t.height)throw std::runtime_error("Changing video dimensions are unsupported");
        if(!t.pts.empty()&&pts<=t.pts.back())throw std::runtime_error("Non-increasing video timestamps");
        t.pts.push_back(pts);
        t.tail=0;
        if(fields.count("pkt_duration_time")&&fields.at("pkt_duration_time")!="N/A")
            t.tail=timestamp_us(fields.at("pkt_duration_time"));
    }
    if(t.pts.empty())throw std::runtime_error("No decoded video frames");
    if(t.tail<=0&&t.pts.size()>1)t.tail=t.pts.back()-t.pts[t.pts.size()-2];
    if(t.tail<=0)throw std::runtime_error("Unknown final frame duration");
    return t;
}
inline bool cfr(const Timeline& t){
    if(t.pts.size()<3)return false;
    auto span=t.pts.back()-t.pts.front();
    auto count=static_cast<std::int64_t>(t.pts.size()-1);
    if(span<=0)return false;
    auto step=span/count;
    if(std::abs(t.tail-step)>2000)return false;
    for(size_t i=1;i<t.pts.size();++i)
        if(std::abs((t.pts[i]-t.pts[i-1])-step)>2000)return false;
    return true;
}
inline std::string temporal_chain(){
    return "format=yuv444p,atadenoise=0a=0.015:0b=0.03:1a=0.012:1b=0.025:2a=0.012:2b=0.025:s=5:a=s";
}
inline std::string stabilize_chain(){
    return "format=yuv444p,deshake=rx=16:ry=16:edge=mirror:blocksize=8:contrast=80:search=exhaustive";
}
inline std::int64_t target_frames(const Timeline& t,int fps){
    return (t.duration()*fps+500000)/1000000;
}
inline std::string interpolate_chain(const Timeline& t,int fps){
    if(!cfr(t)||std::abs(t.pts.front())>2000||t.width<32||t.height<32)
        throw std::runtime_error("Motion interpolation requires near-zero-start CFR video, at least 3 frames and 32x32 pixels");
    auto count=target_frames(t,fps);
    if(count<1)throw std::runtime_error("Invalid target frame count");
    if(count<static_cast<std::int64_t>(t.pts.size()))
        throw std::runtime_error("Target FPS is lower than source FPS; frame reduction is not supported");
    return "format=yuv444p,tpad=stop_mode=clone:stop=3,minterpolate=fps="+std::to_string(fps)
        +":mi_mode=mci:mc_mode=aobmc:me_mode=bilat:me=epzs:vsbmc=1:scd=fdiff:scd_threshold=10,trim=end_frame="+std::to_string(count);
}
inline void validate_video(const Timeline& a,const Timeline& b,int scale,int fps){
    if(static_cast<std::int64_t>(a.width)*scale!=b.width||static_cast<std::int64_t>(a.height)*scale!=b.height)
        throw std::runtime_error("Video output dimensions mismatch");
    auto wanted=fps?target_frames(a,fps):static_cast<std::int64_t>(a.pts.size());
    if(static_cast<std::int64_t>(b.pts.size())!=wanted)
        throw std::runtime_error("Video output frame count mismatch");
    auto tolerance=std::max(a.tail,b.tail)+2000;
    if(std::abs(a.duration()-b.duration())>tolerance)
        throw std::runtime_error("Video output duration mismatch");
    if(fps){
        for(size_t i=0;i<b.pts.size();++i){
            auto expected=static_cast<std::int64_t>(i)*1000000/fps;
            if(std::abs(b.pts[i]-b.pts.front()-expected)>2000)
                throw std::runtime_error("Video output FPS cadence mismatch");
        }
    }else{
        for(size_t i=0;i<a.pts.size();++i)
            if(std::abs((a.pts[i]-a.pts.front())-(b.pts[i]-b.pts.front()))>2000)
                throw std::runtime_error("Video output timestamp cadence mismatch");
    }
}
inline std::string video_report(const Timeline& a,const Timeline& b,int fps){
    std::ostringstream s;
    s<<"{\"video_guard\":\"timeline\",\"accepted\":true,\"input_frames\":"<<a.pts.size()
      <<",\"output_frames\":"<<b.pts.size()<<",\"input_duration_us\":"<<a.duration()
      <<",\"output_duration_us\":"<<b.duration()<<",\"target_fps\":"<<fps<<"}";
    return s.str();
}
}
