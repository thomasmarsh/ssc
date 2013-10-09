#include "asteroid.h"
#include "draw.h"

const double ASTEROID_MASS = 1000;

Asteroid::Asteroid(double sz)
	: ScreenObject(ASTEROID_TYPE,
		       ((sz-50) > 0) ? ((int) sz-50):10, // radius
		       ASTEROID_MASS,
		       100, // maxspeed
		       rand() % Screen::maxX(), rand() % Screen::maxY(), 0,
		       0, 0, 0),
	  mFirstDraw(true)
{
	setup(sz, -1, -1);
	rot.set(drand48() * D_PI, drand48() * D_PI, drand48() * D_PI);
	rot_amt.set((drand48() - .5) / 5, (drand48() - .5) / 5, (drand48() - .5) / 5);

	setState(ALIVE);
}


inline void pushTriangle(std::vector<Triangle> &tri,
	 		 double x1, double y1, double z1,
	 		 double x2, double y2, double z2,
	 		 double x3, double y3, double z3)
{
	Triangle tt;
	tt.v1.vertex.set(x1, y1, z1);
	tt.v2.vertex.set(x2, y2, z2);
	tt.v3.vertex.set(x3, y3, z3);

	tt.calculateNormal();
	tri.push_back(tt);
}

void Asteroid::setup(double size, int iter, int seed)
{
	std::vector<Triangle> &tri = mMesh.mTriangles;

	tri.clear();
	pushTriangle(tri,  1.02,  1,  1, -1.07, -1,  1,  1.03, -1, -1);
	pushTriangle(tri, -1.07, -1,  1,  1.02,  1,  1, -1.09,  1, -1);
	pushTriangle(tri,  1.03, -1, -1, -1.07, -1,  1, -1.09,  1, -1);
	pushTriangle(tri, -1.09,  1, -1,  1.02,  1,  1,  1.03, -1, -1);

	mMesh.normalize();

	if (iter < 0)
	{
		iter = 2;
		if (size > SMALL) iter++;
		if (size > MEDIUM) iter++;
	}

	if (seed == -1)
		seed = rand() + time(NULL);

	for (int i = 0; i < iter; i++)
		split(seed+i, .75);

	mMesh.smooth();
	mMesh.normalize();
	mSize = size;
	mMesh.setMapMode(Mesh::CYLINDRICAL);
}

inline
Coord3<double> Asteroid::midpoint(Coord3<double> &p0,
				  Coord3<double> &p1,
				  Coord3<double> &bc,
				  int seed, double strength)
{
	static Coord3<double> swap, tmp, mp; 
	bool swapped = false;

	if (p0 < p1)
	{
		swap = p0;
		p0 = p1;
		p1 = swap;
		swapped = true;
	}

	mp = (p0 + p1) / 2;

	float r = (distance(p0, bc) + distance(p1, bc)) / 2; // radius
	float l = distance(p0, p1); // edge length
	l *= strength;
	srand48(seed + p0.hash() + p1.hash());
	float d = drand48() - 0.5;
	tmp = mp - bc;
	mp = bc + tmp.normalize() * (r+d*l/2);
	if (swapped)
	{
		swap = p0;
		p0 = p1;
		p1 = swap;
	}
	return mp;
}

void Asteroid::split(int seed, double strength)
{
	Coord3<double> bc(0, 0, 0); // barycenter of asteroid

	std::vector<Triangle>::iterator i = mMesh.mTriangles.begin();
	for (; i != mMesh.mTriangles.end(); ++i)
		for (int j = 0; j < 3; j++)
			bc += (*i)[j].vertex;

	bc /= mMesh.mTriangles.size() * 3;

	Mesh mesh;
	Coord3<double> mp01, mp02, mp12;

	for (i = mMesh.mTriangles.begin();  i!= mMesh.mTriangles.end(); i++)
	{
		mp01 = midpoint((*i).v1.vertex, (*i).v2.vertex, bc, seed, strength);
		mp02 = midpoint((*i).v1.vertex, (*i).v3.vertex, bc, seed, strength);
		mp12 = midpoint((*i).v2.vertex, (*i).v3.vertex, bc, seed, strength);

		mesh.mTriangles.push_back(Triangle((*i).v1.vertex, mp01, mp02));
		mesh.mTriangles.push_back(Triangle((*i).v2.vertex, mp12, mp01));
		mesh.mTriangles.push_back(Triangle((*i).v3.vertex, mp02, mp12));
		mesh.mTriangles.push_back(Triangle(mp12,mp02,mp01));
	}
	mMesh = mesh;
}

unsigned int ASTEROID_TEXTURE = 0;
static bool BuildTexture = true;

#include "glpng.h"

void Asteroid::draw()
{
	if (BuildTexture)
	{
		pngInfo info;
		glGenTextures(1, &ASTEROID_TEXTURE);
		glBindTexture(GL_TEXTURE_2D, ASTEROID_TEXTURE);
		glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_NEAREST);
		glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_NEAREST);
		glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP);
		glTexParameteri(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP);

		if (!pngLoad("/usr/local/share/ssc/ast.png", PNG_NOMIPMAP, PNG_SOLID, &info))
		{
			fprintf(stderr, "Can't load file\n");
			abort();
		}
		BuildTexture = false;
	}
	mMesh.setTexture(ASTEROID_TEXTURE);

	if (mFirstDraw)
	{
		mDisplayList = glGenLists(1);
		glNewList(mDisplayList, GL_COMPILE);
		mMesh.draw();
		glEndList();
		mFirstDraw = false;
	}

	glPushMatrix();
	glTranslated(mPosition.x, -mPosition.y, mPosition.z);
	glRotated(DEG(rot.x), 1, 0, 0);
	glRotated(DEG(rot.y), 0, 1, 0);
	glScalef(mSize, mSize, mSize);
	glCallList(mDisplayList);
	glPopMatrix();
}
