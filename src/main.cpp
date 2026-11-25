#include <algorithm>
#include "analysis.hpp"
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
            std::cout << "DME 0.2.0\n"; return 0;
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
            std::cout<<dme::report(m,dme::plan(m,factor))<<'\n';
            return 0;
        }
        if (argc < 3 || argc > 5) {
            std::cerr << "Usage: dme INPUT OUTPUT [1|2|4|8] [auto|clean|detail|resize|restore|denoise]\n"
                      << "Output: PNG for images; MKV (lossless FFV1/FLAC) or MP4 (H.264/AAC) for video.\n";
            return 2;
        }
        fs::path input = fs::absolute(argv[1]), output = fs::absolute(argv[2]);
        if (!fs::is_regular_file(input) || fs::exists(output))
            throw std::runtime_error("Input must exist; output must not already exist.");
        const std::string factor = argc >= 4 ? argv[3] : "2";
        const std::string preset = argc == 5 ? argv[4] : "auto";
        if (!one_of(factor, {"1","2","4","8"}) || !one_of(preset, {"auto","clean","detail","resize","restore","denoise"}))
            throw std::runtime_error("Invalid scale or preset.");
        const auto in = extension(input), out = extension(output);
        const bool image = one_of(in,{".png",".jpg",".jpeg",".bmp",".tif",".tiff",".webp"});
        if (!image && !one_of(in,{".mp4",".mkv",".mov",".avi",".webm"}))
            throw std::runtime_error("Unsupported input extension.");
        if ((image && out != ".png") || (!image && !one_of(out,{".mkv",".mp4"})))
            throw std::runtime_error("Unsupported output extension for media type.");
        std::string filter;
        if (preset == "auto") {
            auto m=dme::analyze(input.string(),image);
            filter=dme::plan(m,factor);
            std::cerr<<dme::report(m,filter)<<'\n';
        } else {
            if (preset == "clean") filter = "hqdn3d=1.5:1.5:0:0,";
            if (preset == "denoise") filter = "nlmeans=s=2:p=5:r=9,";
            if (preset == "restore") filter = "deblock=filter=weak:block=8:planes=7,deband=1thr=0.012:2thr=0.012:3thr=0.012:range=-8:direction=-1.570796,hqdn3d=1.5:1.5:0:0,";
            filter += "scale=iw*"+factor+":ih*"+factor+":flags=lanczos+accurate_rnd+bitexact";
            if (preset != "resize" && preset != "denoise") filter += ",unsharp=5:5:0.35:5:5:0";
        }
        std::vector<std::string> args = {"ffmpeg","-nostdin","-hide_banner","-loglevel","error",
            "-n","-cpuflags","0","-threads","1","-fflags","+bitexact","-protocol_whitelist","file,pipe","-i",input.string(),
            "-map","0:v:0","-map_metadata","-1","-map_chapters","-1","-filter_threads","1",
            "-vf",filter,"-threads","1","-flags:v","+bitexact"};
        auto append = [&](std::initializer_list<std::string> v){args.insert(args.end(),v);};
        if (image) append({"-frames:v","1","-an","-c:v","png","-pix_fmt","rgba64be"});
        else if (out == ".mkv") append({"-map","0:a?","-c:v","ffv1","-level","3",
            "-pix_fmt","yuv444p16le","-c:a","flac","-flags:a","+bitexact","-fflags","+bitexact"});
        else append({"-map","0:a?","-c:v","libx264","-preset","slow","-crf","16",
            "-pix_fmt","yuv444p","-x264-params","threads=1:lookahead-threads=1",
            "-c:a","aac","-b:a","192k","-flags:a","+bitexact","-fflags","+bitexact"});
        args.push_back(output.string());
        return execute(std::move(args));
    } catch (const std::exception& e) {
        std::cerr << "Error: " << e.what() << '\n'; return 2;
    }
}
