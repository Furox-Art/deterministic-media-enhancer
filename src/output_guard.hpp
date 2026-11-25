#pragma once
#include "optimizer.hpp"
#include <filesystem>
#include <cstdint>
#include <limits>

namespace dme {
namespace fs=std::filesystem;
inline constexpr size_t max_image_pixels=64ULL*1024*1024;
inline Frame decode_full_image(const fs::path& path){
    auto data=capture({"ffmpeg","-nostdin","-hide_banner","-loglevel","error","-xerror","-cpuflags","0",
        "-threads","1","-fflags","+bitexact","-protocol_whitelist","file,pipe","-max_pixels","67108864",
        "-i",path.string(),"-map","0:v:0","-filter_threads","1","-vf","format=gray","-frames:v","1",
        "-threads","1","-c:v","pgm","-f","image2pipe","pipe:1"},max_image_pixels+1024);
    size_t pos=0;
    auto token=[&](){
        while(pos<data.size()&&(data[pos]==' '||data[pos]=='\n'||data[pos]=='\r'||data[pos]=='\t'))++pos;
        size_t start=pos;
        while(pos<data.size()&&data[pos]!=' '&&data[pos]!='\n'&&data[pos]!='\r'&&data[pos]!='\t')++pos;
        return std::string(data.begin()+start,data.begin()+pos);
    };
    if(token()!="P5")throw std::runtime_error("Invalid output image");
    int w=std::stoi(token()),h=std::stoi(token()),max=std::stoi(token());
    if(w<1||h<1||max!=255||pos>=data.size()||data[pos++]!='\n')
        throw std::runtime_error("Invalid output image header");
    size_t n=static_cast<size_t>(w)*static_cast<size_t>(h);
    if(n>max_image_pixels||n!=data.size()-pos)
        throw std::runtime_error("Output image exceeds QA limit or is truncated");
    data.erase(data.begin(),data.begin()+static_cast<std::ptrdiff_t>(pos));
    return {w,h,std::move(data)};
}
struct FullGuard {bool accepted=true; int tiles=0,rejected=0,max_mae=0,max_edge_loss=0,max_clipping=0;};
inline FullGuard validate_full(const Frame& baseline,const Frame& result){
    if(baseline.w!=result.w||baseline.h!=result.h||
        baseline.pixels.size()!=result.pixels.size()||baseline.pixels.empty())
        throw std::runtime_error("Full image output dimensions differ from baseline");
    FullGuard g;
    constexpr int tile=256, stride=240;
    for(int y=0;y<baseline.h;y+=stride)for(int x=0;x<baseline.w;x+=stride){
        int w=std::min(tile,baseline.w-x),h=std::min(tile,baseline.h-y);
        Frame a{w,h,{}},b{w,h,{}};
        a.pixels.reserve(static_cast<size_t>(w)*h);b.pixels.reserve(static_cast<size_t>(w)*h);
        for(int row=0;row<h;++row){
            size_t start=static_cast<size_t>(y+row)*baseline.w+x;
            a.pixels.insert(a.pixels.end(),baseline.pixels.begin()+start,baseline.pixels.begin()+start+w);
            b.pixels.insert(b.pixels.end(),result.pixels.begin()+start,result.pixels.begin()+start+w);
        }
        auto m=measure(a.pixels.data(),w,h);
        auto c=compare(a,b,m);
        ++g.tiles;
        // Block ratios become unreliable at non-native resolution; do not gate on them here.
        bool accepted=c.mae<=500&&c.edge_loss<=12&&c.edge_loss>=-25&&c.clipping<=50&&c.noise_delta>=-40;
        if(!accepted){++g.rejected;g.accepted=false;}
        g.max_mae=std::max(g.max_mae,c.mae);
        g.max_edge_loss=std::max(g.max_edge_loss,c.edge_loss);
        g.max_clipping=std::max(g.max_clipping,c.clipping);
    }
    return g;
}
inline std::string guard_report(const FullGuard& g){
    std::ostringstream s;
    s<<"{\"output_guard\":\"full_image_luma\",\"accepted\":"<<(g.accepted?"true":"false")
      <<",\"tiles\":"<<g.tiles<<",\"rejected_tiles\":"<<g.rejected
      <<",\"max_mae_x100\":"<<g.max_mae<<",\"max_edge_loss_percent\":"<<g.max_edge_loss
      <<",\"max_new_clipping_x100_percent\":"<<g.max_clipping<<"}";
    return s.str();
}
class OutputStage {
    fs::path directory_;
public:
    explicit OutputStage(const fs::path& output){
        std::string pattern=(output.parent_path()/".dme-stage-XXXXXX").string();
        std::vector<char> chars(pattern.begin(),pattern.end());chars.push_back('\0');
        auto* result=mkdtemp(chars.data());
        if(!result)throw std::runtime_error("Could not create output staging directory");
        directory_=result;
    }
    OutputStage(const OutputStage&)=delete;
    OutputStage& operator=(const OutputStage&)=delete;
    ~OutputStage(){std::error_code ec;fs::remove_all(directory_,ec);}
    fs::path file(const std::string& name)const{return directory_/name;}
    void publish(const fs::path& candidate,const fs::path& output)const{
        // Atomic no-replace publication on a filesystem supporting hard links.
        fs::create_hard_link(candidate,output);
    }
};
}
