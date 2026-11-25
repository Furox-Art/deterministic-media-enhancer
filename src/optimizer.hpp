#pragma once
#include "analysis.hpp"
#include "restoration.hpp"

namespace dme {
struct Frame {int w,h; std::vector<unsigned char> pixels;};
inline std::vector<Frame> decode_samples(const std::string& path,bool still,const std::string& chain){
    std::string vf=still?"":"select=not(mod(n\\,15)),";
    vf+="crop=min(iw\\,384):min(ih\\,384):trunc((iw-ow)/16)*8:trunc((ih-oh)/16)*8:exact=1";
    if(!chain.empty()) vf+=","+chain;
    vf+=",format=gray";
    auto data=capture({"ffmpeg","-nostdin","-hide_banner","-loglevel","error","-cpuflags","0","-threads","1",
        "-fflags","+bitexact","-protocol_whitelist","file,pipe","-t","10","-i",path,"-map","0:v:0",
        "-filter_threads","1","-vf",vf,"-frames:v",still?"1":"4","-fps_mode","passthrough",
        "-threads","1","-c:v","pgm","-f","image2pipe","pipe:1"});
    size_t pos=0;
    auto token=[&](){
        while(pos<data.size() && (data[pos]==' '||data[pos]=='\n'||data[pos]=='\r'||data[pos]=='\t')) ++pos;
        size_t start=pos;
        while(pos<data.size() && data[pos]!=' '&&data[pos]!='\n'&&data[pos]!='\r'&&data[pos]!='\t') ++pos;
        return std::string(data.begin()+start,data.begin()+pos);
    };
    std::vector<Frame> frames;
    while(pos<data.size()){
        if(token()!="P5") throw std::runtime_error("Invalid optimization frame");
        int w=std::stoi(token()),h=std::stoi(token()),max=std::stoi(token());
        if(w<1||h<1||w>384||h>384||max!=255||pos>=data.size()||data[pos++]!='\n')
            throw std::runtime_error("Invalid optimization header");
        size_t n=static_cast<size_t>(w)*h;
        if(n>data.size()-pos) throw std::runtime_error("Truncated optimization frame");
        frames.push_back({w,h,{data.begin()+pos,data.begin()+pos+n}});pos+=n;
    }
    if(frames.empty()) throw std::runtime_error("No optimization frames");
    return frames;
}
struct Comparison {
    bool accepted=true;
    int gain=0, mae=0, edge_loss=0, noise_delta=0, block_delta=0, sharp_gain=0, clipping=0;
};
inline Comparison compare(const Frame& a,const Frame& b,const Metrics& m){
    if(a.w!=b.w||a.h!=b.h||a.pixels.size()!=b.pixels.size()||a.pixels.empty())
        throw std::runtime_error("Sample dimensions differ");
    std::int64_t diff=0, flatA=0,flatB=0, nFlat=0, edgeA=0, edgeB=0, nEdge=0, clip=0;
    for(size_t i=0;i<a.pixels.size();++i){
        int x=a.pixels[i],y=b.pixels[i];diff+=std::abs(x-y);
        if(x>4&&x<251&&(y<=1||y>=254))++clip;
    }
    for(int y=1;y<a.h-1;++y)for(int x=1;x<a.w-1;++x){
        int i=y*a.w+x,w=a.w;
        const auto& p=a.pixels;const auto& q=b.pixels;
        int g=std::abs(p[i-1]-p[i+1])+std::abs(p[i-w]-p[i+w]);
        int h=std::abs(q[i-1]-q[i+1])+std::abs(q[i-w]-q[i+w]);
        if(g<=12){
            flatA+=std::abs(4*p[i]-p[i-1]-p[i+1]-p[i-w]-p[i+w]);
            flatB+=std::abs(4*q[i]-q[i-1]-q[i+1]-q[i-w]-q[i+w]);++nFlat;
        }
        if(g>=24){edgeA+=g;edgeB+=h;++nEdge;}
    }
    Comparison c;
    c.mae=static_cast<int>(diff*100/a.pixels.size());
    c.clipping=static_cast<int>(clip*10000/a.pixels.size());
    if(nFlat)c.noise_delta=static_cast<int>((flatA-flatB)*100/(4*nFlat));
    if(edgeA)c.edge_loss=static_cast<int>((edgeA-edgeB)*100/edgeA);
    if(nEdge)c.sharp_gain=static_cast<int>((edgeB-edgeA)*100/nEdge);
    auto ma=measure(a.pixels.data(),a.w,a.h),mb=measure(b.pixels.data(),b.w,b.h);
    c.block_delta=std::max(0,ma.block-100)-std::max(0,mb.block-100);
    c.accepted=c.mae<=500 && c.edge_loss<=12 && c.edge_loss>=-25 && c.clipping<=50
        && c.noise_delta>=-40 && c.block_delta>=-25;
    c.gain=(m.noise>=180&&m.flat>=20?2*c.noise_delta:0)
        +(m.block>=145&&m.edge>=24?3*c.block_delta:0)
        +(m.edge>=24&&m.curvature<55&&m.noise<350?std::clamp(c.sharp_gain,0,800)/4:0)
        -c.mae/2-5*std::max(0,c.edge_loss);
    return c;
}
struct Candidate {std::string name,chain; bool accepted; int score;};
struct Optimization {std::vector<Candidate> candidates; size_t selected=0; std::string filter;};
inline std::vector<std::pair<std::string,std::string>> candidates(const Metrics& m){
    std::vector<std::pair<std::string,std::string>> v={{"baseline",""}};
    const bool noise=m.noise>=180&&m.flat>=20, block=m.block>=145&&m.edge>=24,
        soft=m.edge>=24&&m.curvature<55&&m.noise<350;
    const std::string mild="hqdn3d=1.2:1:0:0", strong="hqdn3d=2.5:2:0:0",
        db="deblock=filter=weak:block=8:planes=7", sh="unsharp=5:5:0.25:5:5:0";
    if(noise){v.push_back({"denoise-mild",mild});v.push_back({"denoise-strong",strong});
        v.push_back({"nonlocal","nlmeans=s=2:p=5:r=9"});
        v.push_back({"bilateral","bilateral=sigmaS=1.5:sigmaR=0.08:planes=1"});
        if(m.noise<1000) v.push_back({"regional",regional_chain()});}
    if(block){v.push_back({"deblock",db});v.push_back({"deblock-low",db+":alpha=0.06:beta=0.03"});}
    if(soft){v.push_back({"sharpen-mild",sh});v.push_back({"sharpen-medium","unsharp=5:5:0.45:5:5:0"});
        v.push_back({"deblur",deblur_chain()});}
    std::string combo;
    for(const auto& s:{block?db:"",noise?mild:"",soft?sh:""})if(!s.empty()){if(!combo.empty())combo+=",";combo+=s;}
    if(!combo.empty()&&std::none_of(v.begin(),v.end(),[&](const auto& x){return x.second==combo;}))
        v.push_back({"combined",combo});
    return v;
}
inline Optimization optimize(const std::string& path,bool still,const Metrics& m,const std::string& factor){
    Optimization o;
    auto base=decode_samples(path,still,"");
    for(const auto& [name,chain]:candidates(m)){
        if(chain.empty()){o.candidates.push_back({name,chain,true,0});continue;}
        auto result=decode_samples(path,still,chain);
        if(result.size()!=base.size())throw std::runtime_error("Optimization sample count differs");
        bool accepted=true;int score=0;
        for(size_t i=0;i<base.size();++i){
            auto c=compare(base[i],result[i],m);accepted=accepted&&c.accepted;score+=c.gain;
        }
        score/=static_cast<int>(base.size());
        o.candidates.push_back({name,chain,accepted,score});
        if(accepted&&score>15&&score>o.candidates[o.selected].score)o.selected=o.candidates.size()-1;
    }
    o.filter=o.candidates[o.selected].chain;
    if(!o.filter.empty())o.filter+=",";
    o.filter+="scale=iw*"+factor+":ih*"+factor+":flags=lanczos+accurate_rnd+bitexact";
    return o;
}
inline std::string optimization_report(const Metrics& m,const Optimization& o){
    auto base=report(m,o.filter);base.pop_back();
    std::ostringstream s;s<<base<<",\"selected\":\""<<o.candidates[o.selected].name<<"\",\"candidates\":[";
    for(size_t i=0;i<o.candidates.size();++i){
        const auto& c=o.candidates[i];if(i)s<<',';
        s<<"{\"name\":\""<<c.name<<"\",\"chain\":\""<<c.chain<<"\",\"accepted\":"<<(c.accepted?"true":"false")
         <<",\"score\":"<<c.score<<'}';
    }
    s<<"]}";return s.str();
}
}
