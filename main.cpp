#include <future>
#include <iostream>
#include <iomanip>
#include <png++/png.hpp>
#include <vector>
#include <utility>
#include <unordered_map>
#include <random>
#include <cstdlib>
#include <time.h>
#include <stdio.h>
#include <algorithm>
#include <ctime>
#include <signal.h>
#include <cxxabi.h>

using namespace std;
using namespace png;

#define WIDTH  4096
#define HEIGHT 4096
#define quadTune 64
#define treeTune 4
const int runSize = 4096 * 4096;

// If everything is breaking for a certain point...
// Remove the non-deterministic parts, run to get point hash in error
// And drop it here
//#define CONCERN 20482
// If everything is breaking early on
//#define OCTRACE
// Use Manhattan distance instead of Euclidian (not really any faster)
//#define MANHATTAN
// Only run a few dozen rows of color-space
//#define SMALLRUN
// Options for neighbors
//#define OCTNEIGH
//#define FARNEIGH
// Tweak neighbor selection in bucket
//#define ROTBIAS
// Shuffle rows
//#define LOOSESHUFFLE
// Should help with all the memory muxing
#define POINT_POOL
#define BUCKET_POOL
// Save out every so often
#define SNAPSHOT

// TODO
// Make it FASTER!
//  remove() and add() are the choking points right now
//    remove() doesn't like the moving of the point into the middle
//      looks like it's doing a new and delete for some reason :(
//    add(), dunno yet

class ColorPoint {
public:
  ColorPoint() : ColorPoint(0, 0, 0) {};
  
  ColorPoint(int r, int g, int b)
  : r(r), g(g), b(b) {
    
  };
  
  int distanceTo(const ColorPoint *other) {
    int dr = r - other->r;
    int dg = g - other->g;
    int db = b - other->b;
    
    #ifdef MANHATTAN
    return dr + dg + db;
    #else
    return dr * dr + dg * dg + db * db;
    #endif
  }
  
  rgb_pixel toColor() {
    return rgb_pixel(r, g, b);
  }
  
  int r, g, b;
};

ostream& operator<<(ostream &os, ColorPoint *cp) {
  return os << "Color<" << cp->r << "," << cp->g << "," << cp->b << ">";
}

class SpacePoint;
typedef vector<SpacePoint*> PointList;

class SpacePoint {
public:
  SpacePoint()
  : x(0), y(0), hash(0), written(0) {
    
  }
  
  void getNeighbors(SpacePoint *space, PointList &out) {
    if (x > 0)    out.push_back(space + spaceOffset(x - 1, y));
    if (x < 4095) out.push_back(space + spaceOffset(x + 1, y));
    if (y > 0)    out.push_back(space + spaceOffset(x, y - 1));
    if (y < 4095) out.push_back(space + spaceOffset(x, y + 1));
    
    // Corners
    #ifdef OCTNEIGH
      if (x > 0    && y > 0    ) out.push_back(space + spaceOffset(x - 1, y - 1));
      if (x < 4095 && y > 0    ) out.push_back(space + spaceOffset(x + 1, y - 1));
      if (x > 0    && y < 4095 ) out.push_back(space + spaceOffset(x - 1, y + 1));
      if (x < 4095 && y < 4095 ) out.push_back(space + spaceOffset(x + 1, y + 1));
    #endif
    
    // Two out
    #ifdef FARNEIGH
      if (x > 1)    out.push_back(space + spaceOffset(x - 2, y));
      if (x < 4094) out.push_back(space + spaceOffset(x + 2, y));
      if (y > 1)    out.push_back(space + spaceOffset(x, y - 2));
      if (y < 4094) out.push_back(space + spaceOffset(x, y + 2));
    #endif
  }
  
  static inline uint32_t spaceOffset(uint32_t x, uint32_t y) {
    return y << 12 | x;
  }
  
  uint32_t x, y, hash;
  bool written;
};

ostream& operator<<(ostream &os, SpacePoint *sp) {
  return os << "Point<" << sp->x << "," << sp->y << ">";
}

typedef struct {
  SpacePoint *space;
  ColorPoint *color;
  int idx;
} Point;

typedef vector<Point*> Bucket;

template <class T>
void poolReset(T *t);

template <class T>
class Pool {
public:
  Pool() {
    cout << "Making a pool of " << Tname() << endl;
    pool.reserve(4096);
  }

  ~Pool() {
    // Release everybody
    for (int i = pool.size() - 1; i >= 0; i--) {
      delete pool[i];
      pool.pop_back();
    }
  }

  T* create() {
    T *ret;
    if (pool.empty()) {
      ret = new T;
    } else {
      ret = pool.back();
      pool.pop_back();
    }

    poolReset<T>(ret);

    return ret;
  }

  void release(T *rel) {
    if (pool.size() == pool.capacity()) {
      cout << "  Making a pool bigger, was " << pool.size() << " of " << Tname() << endl;
    }
    pool.push_back(rel);
  }
private:
  vector<T*> pool;

  string Tname() {
    int status;
    string rname = typeid(T).name();
    char *name = abi::__cxa_demangle(rname.c_str(), nullptr, nullptr, &status);
    
    if (status == 0) {
      rname = name;
      free(name);
    }

    return rname;
  }
};

template <>
void poolReset(Point *p) {
  p->space = nullptr;
  p->color = nullptr;
}

template <>
void poolReset(Bucket *b) {
  b->clear();
}


Pool<Point> pp;
Pool<Bucket> bp;

ostream& operator<<(ostream &os, Point *point) {
  return os << "Point<"
    << point->space->x << ","
    << point->space->y << " # "
    << point->color->r << ","
    << point->color->g << ","
    << point->color->b << ">"; 
}

// Nothing good
class Comparer {
public:
  Comparer(ColorPoint *root):root(root) { };
  bool operator()(ColorPoint *a, ColorPoint *b) {
    return a->distanceTo(root) < b->distanceTo(root);
  }
  
  bool operator()(Point *a, Point *b) {
    return a->color->distanceTo(root) < b->color->distanceTo(root);
  }

  bool operator()(const Bucket &a, const Bucket &b) {
    // Working on buckets
    return a[0]->color->distanceTo(root) < b[0]->color->distanceTo(root);
  }
  
  ColorPoint *root;
};

class BB {
public:
  BB() : BB(0, 0, 0, 0, 0, 0) {}
  BB(int lr, int lg, int lb, int ur, int ug, int ub)
  : lr(lr), lg(lg), lb(lb), ur(ur), ug(ug), ub(ub) {
    
  }
  
  bool intersects(const BB &other) {
    return !(ur < other.lr || other.ur < lr) &&
           !(ug < other.lg || other.ug < lg) &&
           !(ub < other.lb || other.ub < lb);
  }
  
  bool contains(const BB &other) {
    return other.ur <= ur && other.lr >= lr &&
           other.ug <= ug && other.lg >= lg &&
           other.ub <= ub && other.lb >= lb;
  }
  
  void setAround(ColorPoint *center, int radius) {
    lr = center->r - radius;
    ur = center->r + radius;
    lg = center->g - radius;
    ug = center->g + radius;
    lb = center->b - radius;
    ub = center->b + radius;
  }
  
  int lr, lg, lb, ur, ug, ub;
};

ostream& operator<<(ostream &os, BB &bb) {
  os << "Bounds< R ∈ [" << bb.lr << ", " << bb.ur << ")  G ∈ [" << bb.lg << ", " << bb.ug << ")  B ∈ [" << bb.lb << ", " << bb.ub << ") >";
  return os;
}

// For our NN search
typedef struct {
  Point *candidate;
  ColorPoint *source;
  int bestDistanceSq;
  BB bounds;
} Search;

class Octree {
public:
  Octree(Octree *parent, uint8_t depth, uint32_t coord,
         int lr = 0,   int lg = 0,   int lb = 0, 
         int ur = 256, int ug = 256, int ub = 256)
  : parent(parent), depth(depth), coord(coord),
    // Bounding box faces
    bounds(lr, lg, lb, ur, ug, ub),
    diameter(256 >> depth),
      radius(128 >> depth) {

    for (int i = 0; i < 8; i++) children[i] = nullptr;
  }
  
  ~Octree() {
    for (int i = 0; i < 8; i++) {
      if (children[i] != nullptr)
        delete children[i];
    }
  }
  
  Octree *parent;
  
  
  void add(Point *point) {
    // Already have this point
    /*if (depth == 0 && pointHash.find(point->space->hash) != pointHash.end()) {
      cout << "skip ";
      return;
    }*/
    
    #ifdef OCTRACE
    if (depth == 0)
      cout << "  Add " << point << endl;
    #endif
    
    /*if (pointHash.find(point->space->hash) != pointHash.end())
      throw "Tried adding point to non-root node but already there";*/
    
    if (depth < treeTune) {
      // Keep going down!
      getOrCreateChild(point->color)->add(point);
    }
    
    #ifdef CONCERN
      int _before = points.size();
    #endif
    
    if (pointHash.find(point->space->hash) == pointHash.end()) {
      // New bucket
      pointHash[point->space->hash] = points.size();

      #ifdef BUCKET_POOL
        Bucket *bucket = bp.create();
        bucket->push_back(point);
        points.push_back(bucket);
      #else
        Bucket *bucket = new Bucket();
        points.push_back(bucket);
        points.back()->push_back(point);
      #endif
      
    } else {
      // Existing bucket
      int hash = point->space->hash;
      int hashIdx = pointHash[hash];
      points[hashIdx]->push_back(point);
    }
    
    #ifdef CONCERN
      if (point->space->hash == CONCERN) {
        cout << point->color << " ";
        printf("mk %d %d=>%d to %d @%d\n", point->space->hash, _before, points.size(), coord, depth);
        printf("  now have %d in bucket\n", points[pointHash[point->space->hash]].size());
      }
    #endif
  }
  
  void remove(Point *point) {
    #ifdef OCTRACE
    if (depth == 0)
      cout << "  Remove " << point << endl;
    #endif
    
    #ifdef CONCERN
      if (point->space->hash == CONCERN) {
        cout << "Concerning rm of " << point->color << " @" << (int)depth << "(" << coord << "): ";
        /*for (auto it : points)
          cout << it[0]->space->hash << " ";*/
        cout << endl;
      }
    #endif
    
    if (pointHash.find(point->space->hash) == pointHash.end()) {
      for (Bucket *bucket : points)
        cout << (*bucket)[0]->space->hash << " ";
      cout << endl;
      cout << point << endl;
      char *woops = new char[512];
      sprintf(woops, "Tried removing non-existing point %d@%d from %d", point->space->hash, depth, coord);
      throw woops;
    }
    
    if (points.size() == 0) {
      char *woops = new char[512];
      sprintf(woops, "Tried removing from empty Octree");
      throw woops;
    }
    
    uint32_t idx = pointHash[point->space->hash];
    Bucket *lastBucket = points.back();
    Bucket *thisBucket = points[idx];

    #ifdef CONCERN
      if (point->space->hash == CONCERN) {
        cout << "preremove have " << thisBucket.size() << " points in " << coord << "@" << (int)depth << endl;
      }
    #endif

    // Remove from children
    // NB, we only want to remove it from each child once, or they complain
    if (depth < treeTune) {
      int mask = 0;
      for (Point *subpoint : (*thisBucket)) {
        int a = addr(subpoint->color);
        int o = 1 << a;
        if ((mask & o) == 0)
          children[a]->remove(point);
        mask |= o;
      }
    }
    
    
    
    
    if (lastBucket->size() == 0) 
      throw "Tried removing an empty bucket";

    #ifdef CONCERN
      if (point->space->hash == CONCERN) {
        cout << "Concerning rm of " << point->color << " @" << (int)depth << "(" << coord << "): ";
        cout << "have " << thisBucket.size() << " Point in da bucket";
        cout << endl;
      }
    #endif

    
    
    #ifdef CONCERN
      int _before = points.size();
    #endif
    

    Point *lastSample = (*lastBucket)[0];

    if (lastSample->space->hash != point->space->hash) {
      // Swap back into middle
      // TODO Optimization point
      //points[idx] = lastBucket;
      swap(points[idx], points[points.size() - 1]);
      pointHash[lastSample->space->hash] = idx;
    }

    #ifdef BUCKET_POOL
      Bucket *out = points.back();
      bp.release(out);
    #else
      delete points.back();
    #endif

    points.pop_back();
    
    pointHash.erase(point->space->hash);
    
    #ifdef CONCERN
      if (point->space->hash == CONCERN) {
        printf("  rm %d %d=>%d in %d @%d\n", point->space->hash, _before, points.size(), coord, depth);
      }
    #endif
  }
  
  Octree* getOrCreateChild(ColorPoint *color) {
    uint8_t caddr = addr(color);
    
    if (children[caddr] == nullptr) {
      // Calculate new bounds
      int clr = (color->r > bounds.lr + radius) ? bounds.lr + radius : bounds.lr;
      int cur = (color->r < bounds.ur - radius) ? bounds.ur - radius : bounds.ur;
      int clg = (color->g > bounds.lg + radius) ? bounds.lg + radius : bounds.lg;
      int cug = (color->g < bounds.ug - radius) ? bounds.ug - radius : bounds.ug;
      int clb = (color->b > bounds.lb + radius) ? bounds.lb + radius : bounds.lb;
      int cub = (color->b < bounds.ub - radius) ? bounds.ub - radius : bounds.ub;

      children[caddr] = new Octree(this, depth + 1, coord | caddr << (18 - 3 * depth),
                                   clr, clg, clb,
                                   cur, cug, cub);
    }
      
    return children[caddr];
  }
  
  Octree* getChild(ColorPoint *color) {
    return children[addr(color)];
  }
  
  uint8_t addr(ColorPoint *color) {
    uint8_t mask = 128 >> depth;
    uint8_t over = 7 - depth;
    uint8_t raddr = (color->r & mask) >> over;
    uint8_t gaddr = (color->g & mask) >> over;
    uint8_t baddr = (color->b & mask) >> over;
    
    return raddr << 2 | gaddr << 1 | baddr;
  }
  
  Point* findNearest(ColorPoint *color) {
    Octree *child = getChild(color);
    
    if (points.size() == 0) {
      char *woops = new char[512];
      sprintf(woops, "Tried findNearest no points at depth %d", depth);
      throw woops;
    }
    
    if (points.size() <= quadTune || child == nullptr || child->points.size() == 0) {
      // Look here if no children, empty children, or thin us
      Point *ret = (*points[0])[0];
      
      // TODO
      if (points.size() > 2000) {
        //cout << "Searching " << points.size() << endl;
        // Do something radical
        ret = nearestInUs(color);
        //ret = (*points[0])[0];
      } else {
        ret = nearestInUs(color);
      }
      
      
      #ifdef CONCERN
        if (ret->space->hash == CONCERN) {
          cout << ret->color << " ";
          printf("Got %d from %d @%d\n", ret->space->hash, coord, depth);
        }
      #endif
      
      #ifdef OCTRACE
        cout << "Retrieved " << ret << endl;
      #endif

      int distance = color->distanceTo(ret->color);
      
      int radiusSq = radius * radius;

      if (depth > 0 && distance > radiusSq) {
        /*cout << "At " << color << ", nearest is " << ret->color << endl;
        //printf("Bounds are R in [%d,%d), G in [%d,%d), B in [%d,%d)\n", lr, ur, lg, ug, lb, ub);
        cout << bounds << endl;
        cout << "Radius is " << radius << " and distance is " << sqrt(distance) << endl;
        cout << "We are at " << (int)depth << " in " << coord << endl;*/

        Search search;
        search.candidate = ret;
        search.source = color;
        search.bestDistanceSq = distance;
        search.bounds.setAround(color, sqrt(distance));
        
        //cout << "Searching in " << search.bounds << endl;
        
        parent->NNSearchUp(search, this);
        
        return search.candidate;

        //raise(SIGINT);
      }

      return ret;
    } else if (child == nullptr || child->points.size() == 0) {
      // START OF A BAD IDEA

      // We have too many points and
      // Child is empty, do a search instead
      // Faster for some non-shuffles than a full search, by far
      Search search;
      search.candidate = (*points[0])[0];
      search.source = color;
      search.bestDistanceSq = color->distanceTo(search.candidate->color);
      search.bounds = bounds;

      if (parent == nullptr) {
        // We are root
        for (int i = 0; i < 8; i++) {
          if (children[i] != nullptr) {
            children[i]->NNSearchDown(search);
          }
        }
      } else {
        // Call up
        NNSearchUp(search, this);
        this->children[0]->addr(search.source);
      }

      return search.candidate;
      // END OF A BAD IDEA
    } else {
      // Look in children
      return child->findNearest(color);
    }
  }
  
  Point* nearestInUs(ColorPoint *color) {
    // This was, different, before...
    int bestDist = 90000000;
    Bucket *bestBucket = points[0];
    
    for (Bucket *bucket : points) {
      int dist = color->distanceTo((*bucket)[0]->color);
      if (dist < bestDist) {
        // New best friend
        bestDist = dist;
        bestBucket = bucket;
      }
    }
    
    //return (*bestBucket)[0];
    #ifdef ROTBIAS
      // Prefer points close to y=2048
      Bucket sorty = *bestBucket;
      sort(sorty.begin(), sorty.end(), [](Point *a, Point *b) {
        //return abs(a->space->y - 2048) < abs(b->space->y - 2048);
        return a->space->y < b->space->y;
      });
      return sorty[0];
    #else
      //return (*bestBucket)[rand() % bestBucket->size()];
      return (*bestBucket)[0];
    #endif
  }
  
  void NNSearchUp(Search &search, Octree *from) {
    // If we aren't in the search space
    if (!search.bounds.intersects(bounds))
      throw "We're searching up the wrong tree!";
    
    //cout << "  up " << coord << "@" << (int)depth << endl;
    
    // Call children not from
    for (int i = 0; i < 8; i++) {
      if (children[i] != nullptr && children[i] != from) {
        // Search child
        // NB search gets changed in this
        children[i]->NNSearchDown(search);
      }
    }
    
    // If the search space is still outside us and we're not root
    if (depth > 0 && !bounds.contains(search.bounds)) {
      parent->NNSearchUp(search, this);
    }
  }
  
  void NNSearchDown(Search &search) {
    // Skip us if not in search space
    if (!search.bounds.intersects(bounds))
      return;
    
    //cout << "    down " << coord << "@" << (int)depth << endl;
    
    if (points.size() == 0) {
      // We have nobody to search, oh well
      return;
      //throw "oops, no points";
    }
    
    if (points.size() <= quadTune) {
      // Search ourselves if thin enough
      //cout << "bottom" << endl;
      Point *ourNearest = nearestInUs(search.source);
      if (search.source->distanceTo(ourNearest->color) < search.bestDistanceSq) {
        // New best friend!
        //cout << "        NEW BEST FRIEND " << ourNearest->color << endl;
        search.candidate = ourNearest;
        search.bestDistanceSq = search.source->distanceTo(ourNearest->color);
        search.bounds.setAround(search.source, sqrt(search.bestDistanceSq));
        
        //cout << sqrt(search.bestDistanceSq) << endl << search.bounds << endl;
      }
    }
    else if (depth < treeTune) {
      // Otherwise, if we're not at the bottom
      // Go further down!
      for (int i = 0; i < 8; i++) {
        if (children[i] != nullptr) {
          children[i]->NNSearchDown(search);
        }
      }
    }
    
    //cout << "      done " << coord << "@" << (int)depth << endl;
  }
  
  void dump() {
    for (auto bucket : points)
      for (auto it : *bucket)
        cout << it << " ";
    cout << endl;
  }
  
  uint8_t depth;
  Octree* children[8];
  //vector<Point*> points;
  vector<Bucket*> points;
  // Point -> index of point in points in this section of color-space
  unordered_map<uint32_t, uint32_t> pointHash;
  uint32_t coord;

  BB bounds;
  int diameter, radius;
};

inline void put(image<rgb_pixel> *image, SpacePoint *point, ColorPoint *color) {
  (*image)[point->y][point->x] = color->toColor();
  #ifdef OCTRACE
    cout << "Draw " << point << " " << color << endl;
  #endif
}

class Colorful {
public:
  Colorful()
  : root(nullptr, 0, 0),
    image(new png::image<rgb_pixel>(4096, 4096)),
    currentPixel(0) {
    fillColorSpace();
    fillSpaceSpace();
  }

  void shuffleColors() {
    cout << "Color shuffle" << endl;

    #ifdef LOOSESHUFFLE
      for (int i = 0; i < 4095; i++) {
        int j = rand() % (4096 - i) + i;
        int ri = 4096 * i, rj = 4096 * j;
        for (int x = 0; x < 4096; x++)
          swap(colors[ri + x], colors[rj + x]);
      }
    #else
      for (int i = 0; i < (4096 * 4096) - 1; i++) {
        //int j = uniform_int_distribution<int>(i, 4096 * 4096)(rng);
        int j = rand() % (4096 * 4096 - i) + i;
        swap(colors[i], colors[j]);
      }
    #endif
  }

  void seedIdx(int x, int y, int cpIdx) {
    cpIdx*=4096;


    if (x < 0 || y < 0 || x >= 4096 || y >= 4096)
      throw "Tried seeding out of bounds";

    if (cpIdx < 0 || cpIdx >= 4096 * 4096)
      throw "Tried seeding colors too far";

    // Move us to the front
    swap(colors[currentPixel], colors[cpIdx]);

    PointList firstNeighbors;
    SpacePoint &firstPoint = space[SpacePoint::spaceOffset(x, y)];

    if (firstPoint.written) {
      cout << &firstPoint << endl;
      throw "Seeding already written point";
    }

    ColorPoint &firstColor = colors[currentPixel];
    firstPoint.getNeighbors(space, firstNeighbors);
    
    for (SpacePoint *it : firstNeighbors) {
      if (!it->written) {
        #ifdef POINT_POOL
          Point *p = pp.create();
          p->space = it;
          p->color = colors + currentPixel;
          root.add(p);
        #else
          root.add(new Point{it, colors + currentPixel});
        #endif
      }
    }
    
    put(image, &firstPoint, &firstColor);
    firstPoint.written = true;

    

    currentPixel++;
  }

  void seed(int x, int y, int r, int g, int b) {
    cout << "Seed " << x << " " << y << endl;    
    int cpIdx = -1;
    for (int i = 0; i < 4096 * 4096; i++) {
      if (colors + i == colorSpace[r << 16 | g << 8 | b]) {
        cpIdx = i;
        break;
      }
    }

    if (cpIdx == -1)
      throw "Failed to find seed color in color space";

    seedIdx(x, y, cpIdx);
  }

  void simulateTo(int idxTo) {
    if (currentPixel == 0)
      throw "Need at least one seed point";

    PointList nextNeighbors;

    int tracking = (16 * 4096) - 1;
    #ifdef SNAPSHOT
      int snapshot = (256 * 4096) - 1;
    #endif
    int lastTrack = clock();

    for (int c = currentPixel; c < idxTo; c++) {
      if ((c & tracking) == 0) {
        double dt = 1.0 * (clock() - lastTrack) / CLOCKS_PER_SEC;
        double pps = (tracking + 1) / dt;
        double ppso = pps / log(root.points.size());


        cout << "At row " << c / 4096
             << " have " << root.points.size() << " open"/* << endl;

        cout*/ << "   "
          << fixed << setprecision(2) << dt << " sec "
          << pps << " px/sec "
          << ppso << " px/sec/ln(open)" << endl;
        lastTrack = clock();
      }
      
      #ifdef SNAPSHOT
        if ((c & snapshot) == 0) {
          cout << "snapshot-" << (c >> 20) << endl;
          //async(launch::async, [c](::image<rgb_pixel> *image) {
          thread([c](::image<rgb_pixel> *image) {
            cout << "snapshot-" << (c >> 20) << endl;
            char *buf = new char[512];
            sprintf(buf, "output/snapshot-%d.png", c >> 20);
            image->write(buf);
            delete [] buf;
          }, image).detach();

          lastTrack = clock();
        }
      #endif

      ColorPoint &at = colors[c];

      Point *next = root.findNearest(&at);
    
      put(image, next->space, &at);
      next->space->written = true;
      
      nextNeighbors.clear();
      next->space->getNeighbors(space, nextNeighbors);

      for (SpacePoint *neighbor : nextNeighbors) {
        // If we haven't been written to...
        if (!neighbor->written) {
          #ifdef POINT_POOL
            //root.add(pp.create(neighbor, &at));
            Point *p = pp.create();
            p->space = neighbor;
            p->color = &at;
            root.add(p);
          #else
            root.add(new Point({ neighbor, &at }));
          #endif
        }
      }

      root.remove(next);

      #ifdef POINT_POOL
        pp.release(next);
      #else
        delete next;
      #endif
    }
  }

  void write(string file) {
    image->write(file);
  }

//private:
  void fillColorSpace() {
    cout << "Filling colors" << endl;

    colors = new ColorPoint[4096 * 4096];
    colorSpace = new ColorPoint*[4096 * 4096];

    for (int r = 0; r < 256; r++) {
      for (int g = 0; g < 256; g++) {
        for (int b = 0; b < 256; b++) {
          ColorPoint *at = colors + (r << 16 | g << 8 | b);
          colorSpace[r << 16 | g << 8 | b] = at;
          at->r = r;
          at->g = g;
          at->b = b;
          
        }
      }
    }
  }

  void fillSpaceSpace() {
    cout << "Filling spaces" << endl;

    space = new SpacePoint[4096 * 4096];
    for (int x = 0; x < 4096; x++) {
      for (int y = 0; y < 4096; y++) {
        SpacePoint &at = space[SpacePoint::spaceOffset(x, y)];
        at.x = x;
        at.y = y;
        at.hash = SpacePoint::spaceOffset(x, y);
      }
    }
  }

  Octree root;
  image<rgb_pixel> *image;

  ColorPoint *colors;
  ColorPoint **colorSpace;
  SpacePoint *space;

  int currentPixel;
};


int main() {
  try {
    //srand(time(NULL));

    Colorful colors;

    colors.shuffleColors();
    
    // XYRGB
    //colors.seed(1024, 1024,   0, 255, 0  );
    //colors.seed(1024, 3192, 255,   0, 0  );
    //colors.seed(3192, 1024,   0,   0, 255);
    
    //colors.seed(2048, 2048, 255, 0, 0);

    colors.seedIdx(2048, 2048, 0);
    
    //Never a dull moment
    /*for (int i = 0; i < 4096 / 16; i ++) {
      colors.seedIdx(16 * i, 16 * i, i);
      
    }

    for (int i = 0; i < 4096 / 16; i ++) {
      colors.seedIdx(4095 - 16 * i, 16 * i, i + 4096 / 16);
    }*/
    

    //colors.simulateTo(4096 * 4096);
    colors.simulateTo(runSize);
    
    colors.write("output/snapshot-final.png");
  } catch (const char* err) {
    cout << "THE END, EVERYBODY DIED!" << endl << "  " << err << endl;
  }
  
  return 0;
}
