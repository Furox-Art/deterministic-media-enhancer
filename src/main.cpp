#include <algorithm>
#include "output_guard.hpp"
#include "video.hpp"
#include <filesystem>
#include <iostream>
#include <stdexcept>
#include <string>
#include <vector>
#include <cctype>
#include <sys/wait.h>
#include <unistd.h>

namespace fs = std::filesystem;

int execute(std::vector<std::string> args) {
    std::vector<char*> argv;
    for (auto& arg : args) argv.push_back(arg.data());
    argv.push_back(nullptr);
    pid_t pid = fork();
    if (pid < 0) throw std::runtime_error("fork failed");
    if (pid == 0) {
        execvp(argv[0], argv.data());
        _exit(127);
    }
    int status = 0;
    if (waitpid(pid, &status, 0) < 0) throw std::runtime_error("waitpid failed");
    return WIFEXITED(status) ? WEXITSTATUS(status) : 1;
}

std::string extension(const fs::path& path) {
    auto ext = path.extension().string();
    std::transform(ext.begin(), ext.end(), ext.begin(), [](unsigned char c){return std::tolower(c);});
    return ext;
}

bool one_of(const std::string& ext, std::initializer_list<const char*> values) {
    return std::any_of(values.begin(), values.end(), [&](auto x){return ext == x;});
}

int main(int argc, char** argv) {
    try {
        if (argc == 2 && std::string(argv[1]) == "--version") {
            std::cout << "DME 0.5.0\n"; return 0;
        }
        if (argc >= 3 && argc <= 4 && std::string(argv[1]) == "--analyze") {
            fs::path input=fs::absolute(argv[2]);
            const std::string factor=argc==4?argv[3]:"2";
            if(!fs::is_regular_file(input) || !one_of(factor,{"1","2","4","8"}))
                throw std::runtime_error("Invalid analysis input or scale.");
            auto ext=extension(input);
            bool still=one_of(ext,{".png",".jpg",".jpeg",".bmp",".tif",".tiff",".webp"});
            if(!still && !one_of(ext,{".mp4",".mkv",".mov",".avi",".webm"}))
                throw std::runtime_error("Unsupported input extension.");
            auto m=dme::analyze(input.string(),still);
            std::cout<<dme::optimization_report(m,dme::optimize(input.string(),still,m,factor))<<'\n';
            return 0;
        }
        if (argc < 3 || argc > 6) {
            std::cerr << "Usage: dme INPUT OUTPUT [1|2|4|8] [auto|clean|detail|resize|restore|denoise|regional|deblur|wavelet|temporal|stabilize] [keep|30|60|120]\n"
                      << "Output: PNG for images; MKV (lossless FFV1/FLAC) or MP4 (H.264/AAC) for video.\n";
            return 2;
        }
        fs::path input = fs::absolute(argv[1]), output = fs::absolute(argv[2]);
        if (!fs::is_regular_file(input) || fs::exists(output))
            throw std::runtime_error("Input must exist; output must not already exist.");
        const std::string factor = argc >= 4 ? argv[3] : "2";
        const std::string preset = argc >= 5 ? argv[4] : "auto";
        const std::string fps_arg = argc == 6 ? argv[5] : "keep";
        if(!one_of(fps_arg,{"keep","30","60","120"})) throw std::runtime_error("Invalid target FPS");
        const int fps=fps_arg=="keep"?0:std::stoi(fps_arg);
        if (!one_of(factor, {"1","2","4","8"}) || !one_of(preset, {"auto","clean","detail","resize","restore","denoise","regional","deblur","wavelet","temporal","stabilize"}))
            throw std::runtime_error("Invalid scale or preset.");
        const auto in = extension(input), out = extension(output);
        const bool image = one_of(in,{".png",".jpg",".jpeg",".bmp",".tif",".tiff",".webp"});
        if (!image && !one_of(in,{".mp4",".mkv",".mov",".avi",".webm"}))
            throw std::runtime_error("Unsupported input extension.");
        if ((image && out != ".png") || (!image && !one_of(out,{".mkv",".mp4"})))
            throw std::runtime_error("Unsupported output extension for media type.");
        if(image&&(fps||preset=="temporal"||preset=="stabilize"))
            throw std::runtime_error("Video-only operation requested for an image");
        dme::Timeline source;
        if(!image) source=dme::inspect_video(input.string());
        std::string filter;
        if (preset == "auto") {
            auto m=dme::analyze(input.string(),image);
            auto opt=dme::optimize(input.string(),image,m,factor);
            filter=opt.filter;
            std::cerr<<dme::optimization_report(m,opt)<<'\n';
        } else if(preset=="temporal"||preset=="stabilize"){
            filter=(preset=="temporal"?dme::temporal_chain():dme::stabilize_chain())+","+dme::resize_chain(factor);
        } else if(preset=="regional"||preset=="deblur"||preset=="wavelet"){
            filter=(preset=="regional"?dme::regional_chain():preset=="deblur"?dme::deblur_chain():dme::wavelet_chain())
                +","+dme::resize_chain(factor);
        } else {
            if (preset == "clean") filter = "hqdn3d=1.5:1.5:0:0,";
            if (preset == "denoise") filter = "nlmeans=s=2:p=5:r=9,";
            if (preset == "restore") filter = "deblock=filter=weak:block=8:planes=7,deband=1thr=0.012:2thr=0.012:3thr=0.012:range=-8:direction=-1.570796,hqdn3d=1.5:1.5:0:0,";
            filter += "scale=iw*"+factor+":ih*"+factor+":flags=lanczos+accurate_rnd+bitexact";
            if (preset != "resize" && preset != "denoise") filter += ",unsharp=5:5:0.35:5:5:0";
        }
        if(fps){
            // Interpolate at source resolution before spatial filtering/upscaling.
            filter=dme::interpolate_chain(source,fps)+","+filter;
        }
        dme::OutputStage stage(output);
        auto encode=[&](const std::string& chain,const fs::path& target){
        std::vector<std::string> args = {"ffmpeg","-nostdin","-hide_banner","-loglevel","error","-xerror",
            "-n","-cpuflags","0","-threads","1","-fflags","+bitexact","-protocol_whitelist","file,pipe","-i",input.string(),
            "-map","0:v:0","-map_metadata","-1","-map_chapters","-1","-filter_threads","1",
            "-vf",chain,"-threads","1","-flags:v","+bitexact"};
        auto append = [&](std::initializer_list<std::string> v){args.insert(args.end(),v);};
        if (image) append({"-frames:v","1","-an","-c:v","png","-pix_fmt","rgba64be"});
        else if (out == ".mkv") append({"-map","0:a?","-c:v","ffv1","-level","3",
            "-pix_fmt","yuv444p16le","-c:a","flac","-flags:a","+bitexact","-fflags","+bitexact"});
        else append({"-map","0:a?","-c:v","libx264","-preset","slow","-crf","16",
            "-pix_fmt","yuv444p","-x264-params","threads=1:lookahead-threads=1",
            "-c:a","aac","-b:a","192k","-flags:a","+bitexact","-fflags","+bitexact"});
        if(!image) append({"-fps_mode","passthrough"});
        args.push_back(target.string());
        return execute(std::move(args));
        };
        auto candidate=stage.file("candidate"+out);
        if(int code=encode(filter,candidate);code!=0) return code;
        if(image){
            auto result=dme::decode_full_image(candidate);
            if(preset=="auto"&&filter!=dme::resize_chain(factor)){
                auto baseline=stage.file("baseline"+out);
                if(int code=encode(dme::resize_chain(factor),baseline);code!=0)return code;
                auto ref=dme::decode_full_image(baseline);
                auto guard=dme::validate_full(ref,result);
                std::cerr<<dme::guard_report(guard)<<'\n';
                if(!guard.accepted){
                    candidate=baseline;
                    std::cerr<<"{\"fallback\":\"baseline\",\"reason\":\"full_image_guard\"}\n";
                }
            }
        } else {
            int code=execute({"ffmpeg","-nostdin","-hide_banner","-loglevel","error","-xerror",
                "-cpuflags","0","-threads","1","-protocol_whitelist","file,pipe","-i",candidate.string(),
                "-map","0:v:0","-map","0:a?","-threads","1","-f","null","-"});
            if(code!=0)return code;
            auto result=dme::inspect_video(candidate.string());
            dme::validate_video(source,result,std::stoi(factor),fps);
            std::cerr<<dme::video_report(source,result,fps)<<'\n';
        }
        stage.publish(candidate,output);
        return 0;
    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << '\n'; return 2;
    }
}
