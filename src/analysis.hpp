#pragma once
#include <algorithm>
#include <cerrno>
#include <cstdint>
#include <cstdlib>
#include <sstream>
#include <string>
#include <vector>
#include <stdexcept>
#include <sys/wait.h>
#include <unistd.h>

namespace dme {
struct Metrics {
    int frames=0, noise=0, block=100, edge=0, curvature=0, flat=0;
};
inline std::vector<unsigned char> capture(std::vector<std::string> args, size_t limit=4*1024*1024) {
    std::vector<char*> argv;
    for(auto& a:args) argv.push_back(a.data());
    argv.push_back(nullptr);
    int fd[2];
    if(pipe(fd)!=0) throw std::runtime_error("pipe failed");
    auto pid=fork();
    if(pid<0){close(fd[0]);close(fd[1]);throw std::runtime_error("fork failed");}
    if(pid==0){
        close(fd[0]);
        if(dup2(fd[1],STDOUT_FILENO)<0) _exit(126);
        close(fd[1]); execvp(argv[0],argv.data()); _exit(127);
    }
    close(fd[1]);
    std::vector<unsigned char> out;
    unsigned char buf[65536];
    bool failed=false;
    while(true){
        auto n=read(fd[0],buf,sizeof(buf));
        if(n<0 && errno==EINTR) continue;
        if(n<0){failed=true;break;}
        if(n==0) break;
        if(static_cast<size_t>(n)>limit-out.size()){failed=true;break;}
        out.insert(out.end(),buf,buf+n);
    }
    close(fd[0]);
    int status=0; pid_t waited;
    do {waited=waitpid(pid,&status,0);} while(waited<0 && errno==EINTR);
    if(failed || waited<0 || !WIFEXITED(status) || WEXITSTATUS(status)!=0)
        throw std::runtime_error("Analysis decode failed");
    return out;
}
inline Metrics measure(const unsigned char* p,int w,int h){
    std::int64_t residual=0, flatCount=0, total=0, boundary=0, inner=0, nb=0, ni=0, grad=0, curv=0, ne=0;
    for(int y=1;y<h-1;++y) for(int x=1;x<w-1;++x){
        int c=p[y*w+x], l=p[y*w+x-1], r=p[y*w+x+1], u=p[(y-1)*w+x], b=p[(y+1)*w+x];
        int g=std::abs(l-r)+std::abs(u-b);
        ++total;
        if(g<=12){
            residual+=std::abs(4*c-l-r-u-b);
            ++flatCount;
        }
        if(g>=24){grad+=g;curv+=std::abs(4*c-l-r-u-b);++ne;}
        auto add=[&](int coord,int delta){
            if(coord%8==0){boundary+=delta;++nb;}else{inner+=delta;++ni;}
        };
        add(x,std::abs(c-l));add(y,std::abs(c-u));
    }
    Metrics m;
    m.frames=1;
    if(flatCount) m.noise=static_cast<int>(residual*100/(flatCount*4));
    if(total) m.flat=static_cast<int>(flatCount*100/total);
    if(nb && ni) m.block=static_cast<int>(((boundary*ni+nb*ni)*100)/(inner*nb+nb*ni));
    if(ne){m.edge=static_cast<int>(grad/ne);m.curvature=static_cast<int>(curv*100/grad);}
    return m;
}
inline Metrics analyze(const std::string& path,bool still){
    std::string filter=still?"":"select=not(mod(n\\,15)),";
    filter+="crop=min(iw\\,512):min(ih\\,512):trunc((iw-ow)/16)*8:trunc((ih-oh)/16)*8:exact=1,format=gray";
    auto data=capture({"ffmpeg","-nostdin","-hide_banner","-loglevel","error","-cpuflags","0","-threads","1",
        "-fflags","+bitexact","-protocol_whitelist","file,pipe","-t","10","-i",path,"-map","0:v:0",
        "-filter_threads","1","-vf",filter,"-frames:v",still?"1":"8","-fps_mode","passthrough",
        "-threads","1","-c:v","pgm","-f","image2pipe","pipe:1"});
    std::vector<Metrics> samples;
    size_t pos=0;
    auto token=[&](){
        while(pos<data.size() && (data[pos]==' '||data[pos]=='\n'||data[pos]=='\r'||data[pos]=='\t')) ++pos;
        size_t start=pos;
        while(pos<data.size() && data[pos]!=' '&&data[pos]!='\n'&&data[pos]!='\r'&&data[pos]!='\t') ++pos;
        return std::string(data.begin()+start,data.begin()+pos);
    };
    while(pos<data.size()){
        if(token()!="P5") throw std::runtime_error("Invalid analysis image");
        int w=std::stoi(token()),h=std::stoi(token()),max=std::stoi(token());
        if(w<1||h<1||w>512||h>512||max!=255||pos>=data.size()||data[pos++]!='\n')
            throw std::runtime_error("Invalid analysis header");
        size_t n=static_cast<size_t>(w)*h;
        if(n>data.size()-pos) throw std::runtime_error("Truncated analysis frame");
        samples.push_back(measure(data.data()+pos,w,h));pos+=n;
    }
    if(samples.empty()) throw std::runtime_error("No analysis frames");
    auto median=[&](int Metrics::*member){
        std::vector<int> v;for(auto& s:samples)v.push_back(s.*member);
        std::sort(v.begin(),v.end());return v[v.size()/2];
    };
    return {static_cast<int>(samples.size()),median(&Metrics::noise),median(&Metrics::block),
            median(&Metrics::edge),median(&Metrics::curvature),median(&Metrics::flat)};
}
inline std::string plan(const Metrics& m,const std::string& factor){
    std::string s;
    if(m.block>=145 && m.edge>=24) s+="deblock=filter=weak:block=8:planes=7,";
    if(m.flat>=20 && m.noise>=180) s+=m.noise>=350?"hqdn3d=2.5:2:0:0,":"hqdn3d=1.2:1:0:0,";
    s+="scale=iw*"+factor+":ih*"+factor+":flags=lanczos+accurate_rnd+bitexact";
    if(m.edge>=24 && m.curvature<55 && m.noise<350) s+=",unsharp=5:5:0.25:5:5:0";
    return s;
}
inline std::string report(const Metrics& m,const std::string& filter){
    std::ostringstream o;
    o<<"{\"frames\":"<<m.frames<<",\"noise_proxy_x100\":"<<m.noise<<",\"block_ratio_x100\":"<<m.block
     <<",\"edge_gradient\":"<<m.edge<<",\"edge_curvature_x100\":"<<m.curvature
     <<",\"flat_percent\":"<<m.flat<<",\"filter\":\""<<filter<<"\"}";
    return o.str();
}
}
