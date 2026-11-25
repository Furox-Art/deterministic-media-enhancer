#pragma once
#include "video.hpp"
#include <array>
#include <csignal>
#include <iostream>
#include <memory>
#include <fcntl.h>

namespace dme {
struct RGBFrame {
    int w,h;
    std::vector<unsigned char> pixels;
    RGBFrame(int width,int height):w(width),h(height),pixels(static_cast<size_t>(width)*height*3){}
    int at(int x,int y,int c)const{
        x=std::clamp(x,0,w-1);y=std::clamp(y,0,h-1);
        return pixels[(static_cast<size_t>(y)*w+x)*3+c];
    }
    int luma(int x,int y)const{return (77*at(x,y,0)+150*at(x,y,1)+29*at(x,y,2)+128)/256;}
};
inline int floor_half(int x){return x>=0?x/2:(x-1)/2;}
inline int interpolate_luma(const RGBFrame& f,int hx,int hy){
    int x=floor_half(hx),y=floor_half(hy),u=hx-2*x,v=hy-2*y;
    return ((2-u)*(2-v)*f.luma(x,y)+u*(2-v)*f.luma(x+1,y)
        +(2-u)*v*f.luma(x,y+1)+u*v*f.luma(x+1,y+1)+2)/4;
}
struct Shift {int dx=0,dy=0,cost=0,zero_cost=0;bool accepted=false;};
inline Shift register_frame(const RGBFrame& ref,const RGBFrame& other){
    if(ref.w!=other.w||ref.h!=other.h||ref.w<32||ref.h<32)return {};
    constexpr int margin=7;
    int step=std::max(2,std::max(ref.w,ref.h)/160);
    auto cost=[&](int dx,int dy){
        std::int64_t sum=0,n=0;
        for(int y=margin;y<ref.h-margin;y+=step)for(int x=margin;x<ref.w-margin;x+=step){
            sum+=std::abs(other.luma(x,y)-interpolate_luma(ref,2*x+dx,2*y+dy));++n;
        }
        return static_cast<int>(sum*100/std::max<std::int64_t>(1,n));
    };
    Shift s;s.zero_cost=cost(0,0);s.cost=s.zero_cost;
    auto try_shift=[&](int x,int y){int c=cost(x,y);
        if(c<s.cost){s.dx=x;s.dy=y;s.cost=c;}
    };
    for(int y=-8;y<=8;y+=2)for(int x=-8;x<=8;x+=2)try_shift(x,y);
    int bx=s.dx,by=s.dy;
    for(int y=by-1;y<=by+1;++y)for(int x=bx-1;x<=bx+1;++x)try_shift(x,y);
    // Reject unrelated scenes and ambiguous/negligible registration gains.
    s.accepted=s.cost<=600&&s.zero_cost-s.cost>=25&&(s.dx%2!=0||s.dy%2!=0);
    return s;
}
inline int cubic_half(const RGBFrame& f,int hx,int hy,int c){
    constexpr std::array<int,4> half={-1,9,9,-1};
    constexpr std::array<int,4> exact={0,16,0,0};
    const auto& wx=hx%2?half:exact;const auto& wy=hy%2?half:exact;
    int x=hx/2,y=hy/2,sum=0;
    for(int j=0;j<4;++j)for(int i=0;i<4;++i)if(wx[i]&&wy[j])
        sum+=wx[i]*wy[j]*f.at(x+i-1,y+j-1,c);
    return std::clamp((sum+128)/256,0,255);
}
struct FusionStats {std::int64_t neighbors=0,samples=0,frames=0;};
inline RGBFrame fuse(const RGBFrame& center,const RGBFrame* prev,const RGBFrame* next,FusionStats& stats){
    RGBFrame out(center.w*2,center.h*2);
    for(int y=0;y<out.h;++y)for(int x=0;x<out.w;++x)for(int c=0;c<3;++c)
        out.pixels[(static_cast<size_t>(y)*out.w+x)*3+c]=static_cast<unsigned char>(cubic_half(center,x,y,c));
    std::array<const RGBFrame*,2> neighbors={prev,next};
    std::array<Shift,2> shifts{};
    for(size_t i=0;i<2;++i)if(neighbors[i]){
        auto s=register_frame(center,*neighbors[i]);
        if(s.accepted){
            auto reverse=register_frame(*neighbors[i],center);
            if(!reverse.accepted||std::abs(s.dx+reverse.dx)>1||std::abs(s.dy+reverse.dy)>1)s.accepted=false;
        }
        shifts[i]=s;
        if(s.accepted)++stats.neighbors;
    }
    // The native center samples (even/even) are never overwritten.
    // For other phases, use observed samples only where local residual is small.
    for(int y=0;y<out.h;++y)for(int x=0;x<out.w;++x){
        if(!(x%2||y%2))continue;
        size_t index=(static_cast<size_t>(y)*out.w+x)*3;
        std::array<int,3> sum={0,0,0};int count=0;
        for(size_t i=0;i<2;++i){
            auto s=shifts[i];if(!s.accepted)continue;
            int hx=x-s.dx,hy=y-s.dy;
            if(hx%2||hy%2||hx<0||hy<0||hx>=2*center.w||hy>=2*center.h)continue;
            int nx=hx/2,ny=hy/2;
            bool good=true;
            // Local checks suppress motion/occlusion contamination.
            for(int c=0;c<3;++c)if(std::abs(neighbors[i]->at(nx,ny,c)-out.pixels[index+c])>18)good=false;
            if(!good)continue;
            for(int c=0;c<3;++c)sum[c]+=neighbors[i]->at(nx,ny,c);
            ++count;
        }
        if(count){
            for(int c=0;c<3;++c)
                out.pixels[index+c]=static_cast<unsigned char>((out.pixels[index+c]+2*sum[c]+(1+2*count)/2)/(1+2*count));
            ++stats.samples;
        }
    }
    ++stats.frames;return out;
}
class RawPipe {
    pid_t pid_=-1;int fd_=-1;
public:
    RawPipe(std::vector<std::string> args,bool reading){
        int f[2];if(pipe2(f,O_CLOEXEC)!=0)throw std::runtime_error("Raw video pipe failed");
        std::vector<char*> argv;for(auto& a:args)argv.push_back(a.data());argv.push_back(nullptr);
        pid_=fork();
        if(pid_<0){close(f[0]);close(f[1]);throw std::runtime_error("Raw video fork failed");}
        if(pid_==0){
            signal(SIGPIPE,SIG_DFL);
            if(reading){close(f[0]);if(dup2(f[1],STDOUT_FILENO)<0)_exit(126);close(f[1]);}
            else{close(f[1]);if(dup2(f[0],STDIN_FILENO)<0)_exit(126);close(f[0]);}
            execvp(argv[0],argv.data());_exit(127);
        }
        if(reading){close(f[1]);fd_=f[0];}else{close(f[0]);fd_=f[1];}
    }
    RawPipe(const RawPipe&)=delete;RawPipe& operator=(const RawPipe&)=delete;
    ~RawPipe(){finish();}
    bool read_frame(RGBFrame& frame){
        size_t offset=0;
        while(offset<frame.pixels.size()){
            auto n=read(fd_,frame.pixels.data()+offset,frame.pixels.size()-offset);
            if(n<0&&errno==EINTR)continue;
            if(n<0)throw std::runtime_error("Raw video read failed");
            if(!n){if(offset)throw std::runtime_error("Truncated raw frame");return false;}
            offset+=static_cast<size_t>(n);
        }
        return true;
    }
    void write_frame(const RGBFrame& frame){
        size_t offset=0;
        while(offset<frame.pixels.size()){
            auto n=write(fd_,frame.pixels.data()+offset,frame.pixels.size()-offset);
            if(n<0&&errno==EINTR)continue;
            if(n<=0)throw std::runtime_error("Raw video write failed");
            offset+=static_cast<size_t>(n);
        }
    }
    int finish()noexcept{
        if(fd_>=0){close(fd_);fd_=-1;}
        if(pid_<0)return 0;
        int status=0;pid_t r;do{r=waitpid(pid_,&status,0);}while(r<0&&errno==EINTR);
        pid_=-1;return r>=0&&WIFEXITED(status)?WEXITSTATUS(status):1;
    }
};
inline std::string native_rate(const std::string& path,const Timeline& t){
    auto bytes=capture({"ffprobe","-v","error","-protocol_whitelist","file,pipe","-select_streams","v:0",
        "-show_entries","stream=avg_frame_rate","-of","csv=p=0",path});
    std::string rate(bytes.begin(),bytes.end());
    while(!rate.empty()&&(rate.back()=='\n'||rate.back()=='\r'))rate.pop_back();
    auto slash=rate.find('/');
    if(slash==std::string::npos||rate.find_first_not_of("0123456789/")!=std::string::npos)
        throw std::runtime_error("Invalid native rate");
    auto n=std::stoll(rate.substr(0,slash)),d=std::stoll(rate.substr(slash+1));
    if(n<=0||d<=0||n>1000000||d>1000000||n>120*d)throw std::runtime_error("Unsupported native rate");
    for(size_t i=0;i<t.pts.size();++i)
        if(std::abs(t.pts[i]-t.pts.front()-static_cast<std::int64_t>(i)*1000000*d/n)>2000)
            throw std::runtime_error("Native rate does not match timeline");
    return rate;
}
inline void multiframe_encode(const std::string& input,const std::string& target,const Timeline& t,bool mkv){
    if(!cfr(t)||std::abs(t.pts.front())>2000||t.width<32||t.height<32||
        static_cast<std::int64_t>(t.width)*t.height>1920LL*1080)
        throw std::runtime_error("Multiframe requires near-zero-start CFR video, 32x32 minimum and at most 2073600 source pixels");
    auto rate=native_rate(input,t);
    // Ignore broken-pipe signals in the parent so failures unwind and staging is cleaned.
    signal(SIGPIPE,SIG_IGN);
    std::vector<std::string> enc={"ffmpeg","-nostdin","-hide_banner","-loglevel","error","-xerror","-n",
        "-cpuflags","0","-threads","1","-fflags","+bitexact","-f","rawvideo","-pixel_format","rgb24",
        "-video_size",std::to_string(t.width*2)+"x"+std::to_string(t.height*2),"-framerate",rate,"-i","pipe:0",
        "-threads","1","-protocol_whitelist","file,pipe","-i",input,"-map","0:v:0","-map","1:a?",
        "-map_metadata","-1","-map_chapters","-1","-filter_threads","1","-threads","1","-flags:v","+bitexact"};
    auto append=[&](std::initializer_list<std::string> v){enc.insert(enc.end(),v);};
    if(mkv)append({"-c:v","ffv1","-level","3","-pix_fmt","yuv444p16le","-c:a","flac"});
    else append({"-c:v","libx264","-preset","slow","-crf","16","-pix_fmt","yuv444p",
        "-x264-params","threads=1:lookahead-threads=1","-c:a","aac","-b:a","192k"});
    append({"-flags:a","+bitexact","-fflags","+bitexact","-fps_mode","passthrough"});enc.push_back(target);
    // Start writer first; close-on-exec prevents a child from holding another pipe open.
    RawPipe writer(std::move(enc),false);
    RawPipe reader({"ffmpeg","-nostdin","-hide_banner","-loglevel","error","-xerror","-cpuflags","0",
        "-threads","1","-fflags","+bitexact","-protocol_whitelist","file,pipe","-noautorotate","-i",input,
        "-map","0:v:0","-an","-filter_threads","1","-threads","1","-pix_fmt","rgb24",
        "-fps_mode","passthrough","-f","rawvideo","pipe:1"},true);
    auto current=std::make_unique<RGBFrame>(t.width,t.height),next=std::make_unique<RGBFrame>(t.width,t.height);
    std::unique_ptr<RGBFrame> previous;
    if(!reader.read_frame(*current))throw std::runtime_error("No source frames");
    FusionStats stats;
    while(true){
        bool more=reader.read_frame(*next);
        auto out=fuse(*current,previous.get(),more?next.get():nullptr,stats);
        writer.write_frame(out);
        if(!more)break;
        previous=std::move(current);current=std::move(next);
        next=std::make_unique<RGBFrame>(t.width,t.height);
    }
    int read_status=reader.finish(),write_status=writer.finish();
    if(read_status||write_status||stats.frames!=static_cast<std::int64_t>(t.pts.size()))
        throw std::runtime_error("Multiframe encode failed or frame count differs");
    std::cerr<<"{\"multiframe\":true,\"frames\":"<<stats.frames<<",\"accepted_neighbors\":"<<stats.neighbors
        <<",\"fused_pixels\":"<<stats.samples<<"}\n";
}
}
